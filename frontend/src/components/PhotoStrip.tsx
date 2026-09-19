import { useEffect, useRef, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Camera, Loader2, X } from 'lucide-react'

import { api } from '@/api/endpoints'
import { fetchImageObjectUrl } from '@/api/client'
import type { Photo } from '@/api/types'
import { cn } from '@/lib/utils'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { ErrorNote } from '@/components/shared'

/**
 * A row of photos attached to one thing — a weigh-in or a recipe — with an
 * upload button and a viewer.
 *
 * This was the weigh-in component; recipes needed the same strip with two
 * differences, so it takes those as props rather than being copied: what it
 * lists and uploads to, and whether the viewer may change it. A shared recipe
 * shows its photos to everyone and lets only its author add or remove one.
 *
 * Uploads are sent at full size and the server downscales and re-encodes them,
 * which is also what drops EXIF — phone photos carry GPS.
 */
export default function PhotoStrip({
  queryKey,
  list,
  upload,
  canEdit,
  size = 'sm',
  label,
  onChange,
}: {
  queryKey: readonly unknown[]
  list: () => Promise<Photo[]>
  upload: (file: File) => Promise<Photo>
  canEdit: boolean
  /** `sm` fits inside a list row; `lg` is for a page about the thing. */
  size?: 'sm' | 'lg'
  /** What a photo is of, for the viewer title and alt text. */
  label: string
  /** Called after an upload or delete, for anything else that should refetch. */
  onChange?: () => void
}) {
  const queryClient = useQueryClient()
  const inputRef = useRef<HTMLInputElement>(null)
  const [viewing, setViewing] = useState<Photo | null>(null)

  const photos = useQuery({ queryKey, queryFn: list })

  const settle = () => {
    queryClient.invalidateQueries({ queryKey })
    onChange?.()
  }

  const add = useMutation({ mutationFn: upload, onSuccess: settle })
  const remove = useMutation({ mutationFn: (id: string) => api.deletePhoto(id), onSuccess: settle })

  const items = photos.data ?? []

  // Nothing to show and no way to add: render nothing rather than an empty
  // strip with a heading over it.
  if (!canEdit && items.length === 0) return null

  return (
    <div className="space-y-2">
      <div className="flex flex-wrap items-center gap-2">
        {items.map((photo) => (
          <Thumbnail
            key={photo.id}
            photo={photo}
            size={size}
            label={label}
            onOpen={() => setViewing(photo)}
            onRemove={canEdit ? () => remove.mutate(photo.id) : undefined}
          />
        ))}

        {canEdit && (
          <>
            <Button
              variant="outline"
              size="sm"
              disabled={add.isPending}
              onClick={() => inputRef.current?.click()}
            >
              {add.isPending ? <Loader2 className="animate-spin" /> : <Camera />}
              {add.isPending ? 'Uploading…' : items.length ? 'Add another' : 'Add photo'}
            </Button>

            <input
              ref={inputRef}
              type="file"
              accept="image/*"
              className="hidden"
              aria-label={`Upload a ${label.toLowerCase()}`}
              onChange={(e) => {
                const file = e.target.files?.[0]
                if (file) add.mutate(file)
                // Reset so picking the same file twice still fires a change event.
                e.target.value = ''
              }}
            />
          </>
        )}
      </div>

      <ErrorNote error={photos.error} />
      <ErrorNote error={add.error} />
      <ErrorNote error={remove.error} />

      <Dialog open={viewing !== null} onOpenChange={(open) => !open && setViewing(null)}>
        <DialogContent className="sm:max-w-3xl">
          <DialogHeader>
            <DialogTitle>{viewing?.caption ?? label}</DialogTitle>
          </DialogHeader>
          {viewing && <FullImage photo={viewing} label={label} />}
        </DialogContent>
      </Dialog>
    </div>
  )
}

/**
 * The API checks visibility on every read, so `<img src>` cannot fetch a photo
 * directly — it sends no Authorization header. Each image is fetched as a blob
 * and shown through an object URL, which has to be revoked to avoid leaking the
 * blob for the life of the tab.
 */
export function useAuthedImage(url: string) {
  const [objectUrl, setObjectUrl] = useState<string | null>(null)
  const [failed, setFailed] = useState(false)

  useEffect(() => {
    let revoked = false
    let created: string | null = null

    fetchImageObjectUrl(url)
      .then((u) => {
        if (revoked) {
          URL.revokeObjectURL(u)
          return
        }
        created = u
        setObjectUrl(u)
      })
      .catch(() => setFailed(true))

    return () => {
      revoked = true
      if (created) URL.revokeObjectURL(created)
    }
  }, [url])

  return { objectUrl, failed }
}

function Thumbnail({
  photo,
  size,
  label,
  onOpen,
  onRemove,
}: {
  photo: Photo
  size: 'sm' | 'lg'
  label: string
  onOpen: () => void
  onRemove?: () => void
}) {
  const { objectUrl, failed } = useAuthedImage(photo.url)

  return (
    <div className="group relative">
      <button
        type="button"
        onClick={onOpen}
        className={cn(
          'focus-visible:ring-ring/50 bg-muted block overflow-hidden rounded-md border outline-none focus-visible:ring-[3px]',
          size === 'lg' ? 'size-36 sm:size-44' : 'size-20',
        )}
        aria-label={photo.caption ?? `Open ${label.toLowerCase()}`}
      >
        {objectUrl ? (
          <img src={objectUrl} alt={photo.caption ?? ''} className="size-full object-cover" />
        ) : (
          <span className="text-muted-foreground grid size-full place-items-center text-[10px]">
            {failed ? 'failed' : '…'}
          </span>
        )}
      </button>
      {onRemove && (
        <Button
          variant="destructive"
          size="icon-sm"
          aria-label="Delete photo"
          onClick={onRemove}
          className="absolute -top-2 -right-2 size-6 opacity-0 transition-opacity group-focus-within:opacity-100 group-hover:opacity-100"
        >
          <X className="size-3" />
        </Button>
      )}
    </div>
  )
}

function FullImage({ photo, label }: { photo: Photo; label: string }) {
  const { objectUrl, failed } = useAuthedImage(photo.url)
  if (failed) return <ErrorNote error={new Error('Could not load that photo.')} />
  if (!objectUrl) return <div className="bg-muted h-64 animate-pulse rounded-md" />
  return (
    <img
      src={objectUrl}
      alt={photo.caption ?? label}
      className="max-h-[70vh] w-full rounded-md object-contain"
    />
  )
}
