import { useState } from 'react'
import { Bot, Check, Copy } from 'lucide-react'

import { Button } from '@/components/ui/button'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'

/**
 * How to point an AI assistant at this instance.
 *
 * The API serves MCP itself at `/mcp`, authenticated with the same keys as
 * everything else, so there is nothing to install on the server side. What a
 * person needs is the URL for *this* instance and the incantation for a
 * client that only speaks stdio — hence the copy buttons. The key itself is
 * never shown here: it exists once, at creation, in the card above.
 */
export default function ConnectAssistantCard() {
  const url = `${window.location.origin}/mcp`

  const direct = JSON.stringify(
    { type: 'http', url, headers: { Authorization: 'Bearer <KEY>' } },
    null,
    2,
  )

  // Two things about mcp-remote: `--header` values may not survive a space
  // in some clients' config parsers, so the value comes from the environment;
  // and `-y` keeps npx from stopping to ask before the client has a chance to
  // read anything.
  const stdio = JSON.stringify(
    {
      mcpServers: {
        'nom-inal': {
          command: 'npx',
          args: ['-y', 'mcp-remote', url, '--header', 'Authorization:${NOM_INAL_AUTH}'],
          env: { NOM_INAL_AUTH: 'Bearer <KEY>' },
        },
      },
    },
    null,
    2,
  )

  const shell = `npx -y mcp-remote ${url} --header "Authorization: Bearer <KEY>"`

  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <Bot className="size-4" /> Connect an AI assistant
        </CardTitle>
        <CardDescription>
          This instance speaks MCP at <code className="font-mono text-xs">{url}</code>. Authenticate
          with an API key from above: a read-only key exposes only the tools that read, a key that
          can make changes exposes them all. Every API endpoint is a tool, plus{' '}
          <code>log_food</code>, <code>today</code>, <code>progress</code> and{' '}
          <code>add_recipe_from_text</code>.
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        <Snippet
          label="Clients that speak Streamable HTTP (Claude Code, Cursor, and most others)"
          text={direct}
        />
        <Snippet
          label="Clients that only speak stdio (Claude Desktop), via mcp-remote"
          text={stdio}
        />
        <Snippet label="Or on the command line" text={shell} />
        <p className="text-muted-foreground text-xs">
          Replace <code>&lt;KEY&gt;</code> with a key from the list above. The assistant acts as
          you, within that key&rsquo;s scope, and revoking the key disconnects it.
        </p>
      </CardContent>
    </Card>
  )
}

function Snippet({ label, text }: { label: string; text: string }) {
  const [copied, setCopied] = useState(false)

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(text)
      setCopied(true)
      setTimeout(() => setCopied(false), 2000)
    } catch {
      // Outside a secure context the clipboard is unavailable; the text is
      // still selectable, so this is not worth an error state.
    }
  }

  return (
    <div className="space-y-1.5">
      <div className="flex items-center justify-between gap-2">
        <span className="text-muted-foreground text-xs">{label}</span>
        <Button type="button" variant="ghost" size="sm" onClick={copy}>
          {copied ? <Check /> : <Copy />}
          {copied ? 'Copied' : 'Copy'}
        </Button>
      </div>
      <pre className="bg-muted overflow-x-auto rounded-md px-3 py-2 font-mono text-xs">{text}</pre>
    </div>
  )
}
