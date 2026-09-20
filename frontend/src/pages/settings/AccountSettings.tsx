import { useEffect, useRef, useState } from 'react'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { Download, FileArchive, LogOut, Upload } from 'lucide-react'

import { api } from '@/api/endpoints'
import type { ImportReport, MergeCount } from '@/api/types'
import { useAuth } from '@/lib/auth'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import {
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@/components/ui/card'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table'
import { ErrorNote } from '@/components/shared'
import AboutCard from '@/components/AboutCard'

/** The kinds of record an import reports on, in the order the file holds them. */
const REPORT_ROWS: { key: keyof Omit<ImportReport, 'profile_updated' | 'notes'>; label: string }[] =
  [
    { key: 'targets', label: 'Targets' },
    { key: 'reminders', label: 'Reminders' },
    { key: 'foods', label: 'Foods' },
    { key: 'recipes', label: 'Recipes' },
    { key: 'diary', label: 'Diary entries' },
    { key: 'weights', label: 'Weigh-ins' },
  ]

function ImportReportView({ report }: { report: ImportReport }) {
  const total = (c: MergeCount) => c.created + c.updated + c.skipped
  return (
    <div className="space-y-3">
      <p className="text-sm">
        Imported.{' '}
        {report.profile_updated
          ? 'The profile was filled in from the file.'
          : 'The profile was already as the file has it.'}
      </p>
      <Table>
        <TableHeader>
          <TableRow>
            <TableHead>Records</TableHead>
            <TableHead className="text-right">Created</TableHead>
            <TableHead className="text-right">Updated</TableHead>
            <TableHead className="text-right">Already here</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {REPORT_ROWS.filter(({ key }) => total(report[key]) > 0).map(({ key, label }) => (
            <TableRow key={key}>
              <TableCell>{label}</TableCell>
              <TableCell className="tabular text-right">{report[key].created}</TableCell>
              <TableCell className="tabular text-right">{report[key].updated}</TableCell>
              <TableCell className="tabular text-right">{report[key].skipped}</TableCell>
            </TableRow>
          ))}
        </TableBody>
      </Table>
      {report.notes.length > 0 && (
        <div className="space-y-1">
          <p className="text-sm font-medium">What could not be done as asked</p>
          <ul className="text-muted-foreground list-disc space-y-0.5 pl-5 text-xs">
            {report.notes.map((note, i) => (
              <li key={i}>{note}</li>
            ))}
          </ul>
        </div>
      )}
    </div>
  )
}

/**
 * Everything the account owns, out as a file and back in from one.
 *
 * The export is one JSON document with no internal ids in it — foods by
 * name and brand, recipes by name, diary entries by date and what was eaten
 * — which is what lets the import merge rather than restore: the same file
 * can be read into the account it came from, into a new account on another
 * instance, or twice, and nothing is duplicated. The CSV form is the diary
 * and the weigh-ins for a spreadsheet, zipped together.
 */
function AccountDataCard() {
  const queryClient = useQueryClient()
  const fileInput = useRef<HTMLInputElement>(null)
  const [report, setReport] = useState<ImportReport | null>(null)

  const download = useMutation({
    mutationFn: (format: 'json' | 'csv') => api.downloadAccountExport(format),
  })

  const importFile = useMutation({
    mutationFn: async (file: File) => {
      let parsed: unknown
      try {
        parsed = JSON.parse(await file.text())
      } catch {
        throw new Error('That file is not JSON. Import the file an export gave you.')
      }
      if (typeof parsed !== 'object' || parsed === null || Array.isArray(parsed)) {
        throw new Error('That file is not an account export.')
      }
      return api.importAccount(parsed)
    },
    onSuccess: (result) => {
      setReport(result)
      // Anything on screen may now be different: targets, recipes, the diary.
      queryClient.invalidateQueries()
    },
  })

  return (
    <Card>
      <CardHeader>
        <CardTitle>Your data</CardTitle>
        <CardDescription>
          Everything this account owns, as a file you keep. No password and no keys are in it;
          photos are listed but not included.
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        <div className="flex flex-wrap items-center gap-3">
          <Button
            variant="outline"
            disabled={download.isPending}
            onClick={() => download.mutate('json')}
          >
            <Download /> Export as JSON
          </Button>
          <Button
            variant="outline"
            disabled={download.isPending}
            onClick={() => download.mutate('csv')}
            title="diary.csv and weights.csv, zipped"
          >
            <FileArchive /> Export as CSV
          </Button>
          <Button
            variant="outline"
            disabled={importFile.isPending}
            onClick={() => fileInput.current?.click()}
          >
            <Upload /> {importFile.isPending ? 'Importing…' : 'Import a file'}
          </Button>
          <input
            ref={fileInput}
            type="file"
            accept="application/json,.json"
            className="sr-only"
            aria-label="Account export to import"
            onChange={(e) => {
              const file = e.target.files?.[0]
              if (file) importFile.mutate(file)
              e.target.value = ''
            }}
          />
        </div>
        <p className="text-muted-foreground text-xs">
          Importing merges: a record already here is left alone or brought up to date, never
          duplicated, and the report says which. The JSON export is the file to import; the CSV is
          for a spreadsheet.
        </p>
        <ErrorNote error={download.error} />
        <ErrorNote error={importFile.error} />
        {report && <ImportReportView report={report} />}
      </CardContent>
    </Card>
  )
}

/**
 * The account itself: who you are to the app, and the way out.
 *
 * Deliberately small. Body facts live under Body, keys under Integrations;
 * this is the name on the header, the email you sign in with, and sign out.
 * The "About" card describing the running build belongs on this page too,
 * below the account card — it is about the instance, not about the diet.
 */
export default function AccountSettings() {
  const { user, setUser, signOut } = useAuth()
  const [name, setName] = useState(user?.display_name ?? '')
  const [saved, setSaved] = useState(false)

  useEffect(() => {
    if (user) setName(user.display_name)
  }, [user])

  const save = useMutation({
    mutationFn: () => api.updateProfile({ display_name: name.trim() }),
    onSuccess: (profile) => {
      setUser(profile)
      setSaved(true)
      setTimeout(() => setSaved(false), 2500)
    },
  })

  return (
    <>
      <Card>
        <CardHeader>
          <CardTitle>Account</CardTitle>
          <CardDescription>
            The name the app greets you by, and the email you sign in with.
          </CardDescription>
          {saved && (
            <CardAction>
              <Badge variant="success">Saved</Badge>
            </CardAction>
          )}
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="grid gap-3 sm:grid-cols-2">
            <div className="space-y-1.5">
              <Label htmlFor="a-name">Name</Label>
              <Input id="a-name" value={name} onChange={(e) => setName(e.target.value)} />
            </div>
            <div className="space-y-1.5">
              <Label htmlFor="a-email">Email</Label>
              <Input id="a-email" value={user?.email ?? ''} disabled />
            </div>
          </div>
          <ErrorNote error={save.error} />
          <div className="flex flex-wrap items-center gap-3">
            <Button
              disabled={save.isPending || !name.trim() || name.trim() === user?.display_name}
              onClick={() => save.mutate()}
            >
              {save.isPending ? 'Saving…' : 'Save name'}
            </Button>
            <Button variant="outline" onClick={signOut}>
              <LogOut /> Sign out
            </Button>
          </div>
        </CardContent>
      </Card>

      <AccountDataCard />

      <AboutCard />
    </>
  )
}
