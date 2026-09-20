import { useQuery } from '@tanstack/react-query'

import { api } from '@/api/endpoints'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'

/**
 * What this instance is running.
 *
 * "Which version are you on?" is the first question in every bug report, and
 * the honest answer is a commit, not a version string: `latest` and `edge`
 * both move. The build stamps its commit and time into the image; a source
 * build that was never stamped says so rather than guessing from a working
 * tree that may not even be a checkout.
 */
export default function AboutCard() {
  const health = useQuery({ queryKey: ['health'], queryFn: () => api.health() })
  const h = health.data

  const stamp = h
    ? h.git_sha
      ? `Build ${h.git_sha.slice(0, 7)}${h.built_at ? ` · built ${h.built_at.slice(0, 10)}` : ''}`
      : 'Source build, not stamped with a commit.'
    : null

  return (
    <Card>
      <CardHeader>
        <CardTitle>About this instance</CardTitle>
        <CardDescription>Quote the build when reporting a problem.</CardDescription>
      </CardHeader>
      <CardContent className="text-muted-foreground space-y-1 text-sm">
        {h && (
          <>
            <p>
              <span className="text-foreground font-medium">nom-inal</span> v{h.version}
            </p>
            <p className="tabular">{stamp}</p>
            <p>
              USDA search {h.usda_configured ? 'is configured.' : 'is off — no USDA_API_KEY set.'}
            </p>
          </>
        )}
        {health.isError && <p>The API did not answer its health check.</p>}
      </CardContent>
    </Card>
  )
}
