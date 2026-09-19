import { useMutation } from '@tanstack/react-query'
import { Check } from 'lucide-react'

import { api } from '@/api/endpoints'
import type { Nutrient } from '@/api/types'
import { useAuth } from '@/lib/auth'
import { NUTRIENTS } from '@/lib/nutrients'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Switch } from '@/components/ui/switch'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import { ErrorNote } from '@/components/shared'

/**
 * Which nutrients to show, and which to chart.
 *
 * One table rather than two lists of eight checkboxes: the two questions are
 * about the same eight things, and putting them side by side makes the
 * difference between them legible — a readout can carry all eight, a page of
 * charts realistically cannot.
 *
 * Saved immediately on toggle. There is nothing to get half-right here and
 * nothing destructive to confirm, so a Save button would only be a second step
 * between wanting fibre on screen and seeing it.
 */
export default function DisplayEditor() {
  const { user, setUser } = useAuth()

  const save = useMutation({
    mutationFn: (body: { shown_nutrients?: Nutrient[]; chart_nutrients?: Nutrient[] }) =>
      api.updateProfile(body),
    // The profile in auth state is what every readout reads, so updating it is
    // what makes the change appear across the app rather than only here.
    onSuccess: (profile) => setUser(profile),
  })

  const shown = user?.shown_nutrients ?? []
  const charted = user?.chart_nutrients ?? []

  const toggle = (list: Nutrient[], key: Nutrient) =>
    list.includes(key) ? list.filter((k) => k !== key) : [...list, key]

  return (
    <Card>
      <CardHeader>
        <CardTitle>What to show</CardTitle>
        <CardDescription>
          Every food already stores all eight of these; this is only about which ones reach the
          screen. Kept on your account, so it follows you between devices.
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-3">
        <ErrorNote error={save.error} />
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Nutrient</TableHead>
              <TableHead className="text-right">In readouts</TableHead>
              <TableHead className="text-right">On the home page</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {NUTRIENTS.map((meta) => (
              <TableRow key={meta.key}>
                <TableCell>
                  <span className="flex items-center gap-2 font-medium">
                    {/* A colour chip, not coloured text: the name stays in
                        normal ink so it is legible at any contrast. */}
                    <span
                      aria-hidden="true"
                      className="size-2.5 shrink-0 rounded-full"
                      style={{ background: meta.color }}
                    />
                    {meta.label}
                  </span>
                  <span className="text-muted-foreground text-xs">{meta.unit}</span>
                </TableCell>
                <TableCell className="text-right">
                  <Switch
                    aria-label={`Show ${meta.label} in readouts`}
                    checked={shown.includes(meta.key)}
                    disabled={save.isPending}
                    onCheckedChange={() => save.mutate({ shown_nutrients: toggle(shown, meta.key) })}
                  />
                </TableCell>
                <TableCell className="text-right">
                  <Switch
                    aria-label={`Chart ${meta.label} on the home page`}
                    checked={charted.includes(meta.key)}
                    disabled={save.isPending}
                    onCheckedChange={() =>
                      save.mutate({ chart_nutrients: toggle(charted, meta.key) })
                    }
                  />
                </TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>

        <p className="text-muted-foreground flex items-center gap-1.5 text-xs">
          {save.isSuccess && !save.isPending && <Check className="size-3.5" />}
          The home page draws these as a share of each one&rsquo;s goal or budget, which is what
          lets kcal and grams share an axis. Switch it to actual figures there and each nutrient
          gets its own small chart instead, because the raw numbers share no scale.
        </p>
      </CardContent>
    </Card>
  )
}
