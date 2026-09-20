import { useCallback, useEffect, useRef, useState } from 'react'
import { Camera, ImageUp, Zap, ZapOff } from 'lucide-react'

import { Alert, AlertDescription } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import { Spinner } from '@/components/shared'

/**
 * The product symbologies a packet carries. Anything else — QR codes, the
 * code on a shipping label — is not a food, and offering to decode it only
 * produces a lookup that fails.
 */
const FORMATS = ['ean_13', 'ean_8', 'upc_a', 'upc_e', 'code_128'] as const

/**
 * The native Shape Detection API, where the browser has it. Typed here
 * because TypeScript's DOM lib does not ship it yet.
 */
interface NativeDetector {
  detect(source: ImageBitmapSource): Promise<{ rawValue: string }[]>
}
interface NativeDetectorCtor {
  new (options: { formats: string[] }): NativeDetector
  getSupportedFormats(): Promise<string[]>
}

/**
 * One interface over two decoders, so the scan loop and the photo path do
 * not care which is underneath.
 *
 * `BarcodeDetector` is used when the browser both has it and reports a
 * product format it can read: the constructor exists on some platforms
 * where the implementation supports nothing at all, and asking is the only
 * way to tell. Everywhere else, ZXing does the work, loaded on first use so
 * the picker does not pay for it until a camera is opened.
 */
interface Decoder {
  fromVideo(video: HTMLVideoElement): Promise<string | null>
  fromImage(url: string): Promise<string | null>
}

async function makeDecoder(): Promise<Decoder> {
  const Native = (globalThis as { BarcodeDetector?: NativeDetectorCtor }).BarcodeDetector
  if (Native) {
    try {
      const supported = await Native.getSupportedFormats()
      const formats = FORMATS.filter((f) => supported.includes(f))
      if (formats.length > 0) {
        const detector = new Native({ formats })
        const first = async (source: ImageBitmapSource) =>
          (await detector.detect(source))[0]?.rawValue ?? null
        return {
          fromVideo: (video) => first(video),
          fromImage: async (url) => {
            const image = await loadImage(url)
            return first(image)
          },
        }
      }
    } catch {
      // Fall through to ZXing: a detector that throws on construction is
      // no better than one that is missing.
    }
  }

  const { BrowserMultiFormatReader, BarcodeFormat } = await import('@zxing/browser')
  const reader = new BrowserMultiFormatReader()
  reader.possibleFormats = [
    BarcodeFormat.EAN_13,
    BarcodeFormat.EAN_8,
    BarcodeFormat.UPC_A,
    BarcodeFormat.UPC_E,
    BarcodeFormat.CODE_128,
  ]
  // A canvas the video is drawn onto for each attempt, kept so it is not
  // allocated a few times a second.
  let canvas: HTMLCanvasElement | null = null
  return {
    fromVideo: async (video) => {
      if (video.readyState < HTMLMediaElement.HAVE_CURRENT_DATA) return null
      canvas ??= document.createElement('canvas')
      canvas.width = video.videoWidth
      canvas.height = video.videoHeight
      const ctx = canvas.getContext('2d', { willReadFrequently: true })
      if (!ctx) return null
      ctx.drawImage(video, 0, 0)
      try {
        return reader.decodeFromCanvas(canvas).getText()
      } catch {
        // NotFoundException: no barcode in this frame. The next one may have it.
        return null
      }
    },
    fromImage: async (url) => {
      try {
        return (await reader.decodeFromImageUrl(url)).getText()
      } catch {
        return null
      }
    },
  }
}

function loadImage(url: string): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const image = new Image()
    image.onload = () => resolve(image)
    image.onerror = () => reject(new Error('The photo could not be read.'))
    image.src = url
  })
}

/** What to tell someone when the camera cannot be opened, by why. */
function cameraFailure(err: unknown): string {
  const name = err instanceof DOMException ? err.name : ''
  if (name === 'NotAllowedError' || name === 'SecurityError') {
    return 'Camera access was refused. Allow the camera for this site in your browser settings, or take a photo of the barcode below instead.'
  }
  if (name === 'NotFoundError' || name === 'OverconstrainedError') {
    return 'No camera was found on this device. You can still take a photo of the barcode below.'
  }
  if (name === 'NotReadableError') {
    return 'The camera is in use by another app. Close it and try again, or use a photo of the barcode.'
  }
  if (typeof navigator === 'undefined' || !navigator.mediaDevices?.getUserMedia) {
    return 'This browser cannot open the camera here — cameras need a secure (https) page. Use a photo of the barcode instead.'
  }
  return err instanceof Error && err.message ? err.message : 'The camera could not be opened.'
}

/** How often the live preview is examined. Twice a frame buys nothing. */
const SCAN_INTERVAL_MS = 120

/**
 * Read a barcode off a packet, with the camera or from a photo.
 *
 * Both routes end in the same place: `onDetected` with the digits, which the
 * caller looks up. The camera is opened only when asked, torn down on unmount,
 * and asked for the rear-facing lens; a torch toggle appears when the track
 * says it has one. A photo — from the file input, which on a phone is the
 * camera app — goes through the same decoder, and is the route that still
 * works when camera access is refused or the page is not secure.
 */
export default function BarcodeScanner({
  onDetected,
  autoStart = false,
}: {
  onDetected: (code: string) => void
  /** Open the camera as soon as the component mounts. */
  autoStart?: boolean
}) {
  const videoRef = useRef<HTMLVideoElement>(null)
  const streamRef = useRef<MediaStream | null>(null)
  const decoderRef = useRef<Promise<Decoder> | null>(null)
  const [scanning, setScanning] = useState(false)
  const [opening, setOpening] = useState(false)
  const [decodingPhoto, setDecodingPhoto] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [torchAvailable, setTorchAvailable] = useState(false)
  const [torchOn, setTorchOn] = useState(false)

  const decoder = () => (decoderRef.current ??= makeDecoder())

  const stop = useCallback(() => {
    streamRef.current?.getTracks().forEach((t) => t.stop())
    streamRef.current = null
    if (videoRef.current) videoRef.current.srcObject = null
    setScanning(false)
    setTorchOn(false)
    setTorchAvailable(false)
  }, [])

  const start = useCallback(async () => {
    setError(null)
    setOpening(true)
    try {
      if (!navigator.mediaDevices?.getUserMedia) throw new Error('no camera api')
      const stream = await navigator.mediaDevices.getUserMedia({
        video: { facingMode: { ideal: 'environment' } },
        audio: false,
      })
      streamRef.current = stream
      const video = videoRef.current
      if (!video) {
        stream.getTracks().forEach((t) => t.stop())
        return
      }
      video.srcObject = stream
      await video.play()

      // `torch` is not in the lib's MediaTrackCapabilities yet.
      const track = stream.getVideoTracks()[0]
      const capabilities = (track?.getCapabilities?.() ?? {}) as { torch?: boolean }
      setTorchAvailable(Boolean(capabilities.torch))
      setScanning(true)
    } catch (err) {
      setError(cameraFailure(err))
      stop()
    } finally {
      setOpening(false)
    }
  }, [stop])

  // The scan loop. It runs while the camera is open and ends itself the
  // moment a code is read, so a packet held up to the lens is looked up
  // once, not once per frame.
  useEffect(() => {
    if (!scanning) return
    let cancelled = false
    let timer: ReturnType<typeof setTimeout> | undefined
    const tick = async () => {
      const video = videoRef.current
      if (cancelled || !video) return
      try {
        const code = await (await decoder()).fromVideo(video)
        if (cancelled) return
        if (code) {
          stop()
          onDetected(code)
          return
        }
      } catch (err) {
        if (!cancelled) setError(err instanceof Error ? err.message : 'Scanning failed.')
      }
      timer = setTimeout(tick, SCAN_INTERVAL_MS)
    }
    void tick()
    return () => {
      cancelled = true
      clearTimeout(timer)
    }
  }, [scanning, stop, onDetected])

  useEffect(() => {
    if (autoStart) void start()
    return stop
  }, [autoStart, start, stop])

  const toggleTorch = async () => {
    const track = streamRef.current?.getVideoTracks()[0]
    if (!track) return
    try {
      // `torch` is a real constraint on Android Chrome; the DOM lib does not
      // know it, hence the cast.
      await track.applyConstraints({
        advanced: [{ torch: !torchOn } as MediaTrackConstraintSet],
      })
      setTorchOn((v) => !v)
    } catch {
      setTorchAvailable(false)
    }
  }

  const fromPhoto = async (file: File | undefined) => {
    if (!file) return
    setError(null)
    setDecodingPhoto(true)
    const url = URL.createObjectURL(file)
    try {
      const code = await (await decoder()).fromImage(url)
      if (code) onDetected(code)
      else
        setError(
          'No barcode was found in that photo. Try a closer, straighter shot with the whole code in frame.',
        )
    } catch (err) {
      setError(err instanceof Error ? err.message : 'The photo could not be read.')
    } finally {
      URL.revokeObjectURL(url)
      setDecodingPhoto(false)
    }
  }

  return (
    <div className="space-y-2">
      {/* The video element is always mounted, hidden until a stream is on it,
          so the stream has somewhere to go the moment it opens. */}
      <div className={scanning ? 'relative overflow-hidden rounded-md border bg-black' : 'hidden'}>
        <video
          ref={videoRef}
          className="aspect-[4/3] w-full object-cover"
          playsInline
          muted
          aria-label="Camera preview"
        />
        <div
          aria-hidden="true"
          className="pointer-events-none absolute inset-x-[12%] top-1/2 h-16 -translate-y-1/2 rounded border-2 border-white/80"
        />
        {torchAvailable && (
          <Button
            type="button"
            size="icon"
            variant="secondary"
            className="absolute top-2 right-2"
            onClick={toggleTorch}
            aria-label={torchOn ? 'Turn the torch off' : 'Turn the torch on'}
            aria-pressed={torchOn}
          >
            {torchOn ? <ZapOff /> : <Zap />}
          </Button>
        )}
      </div>

      <div className="flex flex-wrap gap-2">
        {scanning ? (
          <Button type="button" variant="outline" size="sm" onClick={stop}>
            Stop camera
          </Button>
        ) : (
          <Button type="button" variant="outline" size="sm" onClick={start} disabled={opening}>
            <Camera /> {opening ? 'Opening camera…' : 'Scan with camera'}
          </Button>
        )}
        <Button type="button" variant="outline" size="sm" asChild disabled={decodingPhoto}>
          <label className="cursor-pointer">
            <ImageUp /> {decodingPhoto ? 'Reading photo…' : 'Photo of a barcode'}
            <input
              type="file"
              accept="image/*"
              capture="environment"
              className="sr-only"
              aria-label="Photo of a barcode"
              onChange={(e) => {
                void fromPhoto(e.target.files?.[0])
                // So the same photo can be chosen again after a miss.
                e.target.value = ''
              }}
            />
          </label>
        </Button>
      </div>

      {scanning && <Spinner label="Hold the barcode inside the frame…" />}

      {error && (
        <Alert variant="warning">
          <AlertDescription>{error}</AlertDescription>
        </Alert>
      )}
    </div>
  )
}
