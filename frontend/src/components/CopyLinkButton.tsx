import { useEffect, useState } from 'react'
import { Check, Link2 } from 'lucide-react'

import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'

/**
 * Puts a URL on the clipboard and says so for a moment.
 *
 * The clipboard API needs a secure context and a user gesture; when it is
 * not there — plain http on a LAN, an older browser — the link is shown in a
 * box instead so it can still be selected and copied by hand. Never a silent
 * no-op: a button that says "Copied" when nothing was copied is the worst
 * outcome.
 */
export function CopyLinkButton({ url, label = 'Copy link' }: { url: string; label?: string }) {
  const [state, setState] = useState<'idle' | 'copied' | 'manual'>('idle')

  useEffect(() => {
    if (state !== 'copied') return
    const id = setTimeout(() => setState('idle'), 2000)
    return () => clearTimeout(id)
  }, [state])

  const copy = async () => {
    try {
      if (!navigator.clipboard) throw new Error('no clipboard')
      await navigator.clipboard.writeText(url)
      setState('copied')
    } catch {
      setState('manual')
    }
  }

  if (state === 'manual') {
    return (
      <Input
        readOnly
        value={url}
        aria-label="Link to this recipe"
        className="w-72 max-w-full text-xs"
        onFocus={(e) => e.target.select()}
        autoFocus
      />
    )
  }

  return (
    <Button variant="outline" size="sm" onClick={copy} aria-live="polite">
      {state === 'copied' ? <Check /> : <Link2 />}
      {state === 'copied' ? 'Copied' : label}
    </Button>
  )
}
