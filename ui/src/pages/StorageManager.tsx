import { CheckCircle2, Download, HardDrive, SearchCheck, Trash2 } from 'lucide-react';
import { useState } from 'react';
import {
  backup_delete,
  backup_history,
  backup_manifest,
  backup_verify,
  pick_directory,
  storage_info,
  storage_set_root,
} from '../api/client';
import type { BackupRecord } from '../api/types';
import {
  Badge,
  Button,
  Card,
  CardHeader,
  Dialog,
  EmptyState,
  ErrorBanner,
  Field,
  Input,
  PageLoading,
  Progress,
  StatCard,
  Table,
  TD,
  TH,
  TR,
} from '../components/ui';
import { errMsg, formatBytes, formatDate } from '../lib/format';
import { useAsync } from '../lib/hooks';

const EXPORT_CAP = 50;
const VERIFY_CAP = 20;

function download(filename: string, content: string, mime: string): void {
  const blob = new Blob([content], { type: mime });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  a.click();
  URL.revokeObjectURL(url);
}

export default function StorageManager() {
  const { data, error, loading, reload } = useAsync(
    () => Promise.all([storage_info(), backup_history({ limit: 200 })]),
    [],
  );

  const [root, setRoot] = useState('');
  const [actionError, setActionError] = useState<string | null>(null);
  const [browsing, setBrowsing] = useState(false);

  const [exporting, setExporting] = useState(false);
  const [verifyProgress, setVerifyProgress] = useState<string | null>(null);
  const [verifySummary, setVerifySummary] = useState<
    { ok: number; failed: number; failures: string[] } | null
  >(null);

  const [deleteTarget, setDeleteTarget] = useState<BackupRecord | null>(null);
  const [deleting, setDeleting] = useState(false);

  const info = data?.[0] ?? null;
  const backups = data?.[1] ?? [];

  async function browseRoot() {
    setActionError(null);
    setBrowsing(true);
    try {
      const path = await pick_directory();
      if (path) {
        await storage_set_root({ path });
        setRoot(path);
        reload();
      }
    } catch (e) {
      setActionError(errMsg(e));
    } finally {
      setBrowsing(false);
    }
  }

  async function confirmDelete() {
    if (!deleteTarget) return;
    setDeleting(true);
    setActionError(null);
    try {
      await backup_delete({ backupId: deleteTarget.id });
      setDeleteTarget(null);
      reload();
    } catch (e) {
      setActionError(errMsg(e));
    } finally {
      setDeleting(false);
    }
  }

  async function exportManifests() {
    setActionError(null);
    setExporting(true);
    try {
      const slice = backups.slice(0, EXPORT_CAP);
      const results = await Promise.allSettled(
        slice.map((b) => backup_manifest({ backupId: b.id })),
      );
      const manifests = results
        .filter((r): r is PromiseFulfilledResult<Awaited<ReturnType<typeof backup_manifest>>> =>
          r.status === 'fulfilled',
        )
        .map((r) => r.value);
      const failed = results.length - manifests.length;
      const payload = {
        exported_at: new Date().toISOString(),
        count: manifests.length,
        failed,
        manifests,
      };
      const stamp = new Date().toISOString().slice(0, 10);
      download(
        `gbr-manifests-${stamp}.json`,
        JSON.stringify(payload, null, 2),
        'application/json',
      );
      if (failed > 0) {
        setActionError(`${failed} manifest(s) could not be read and were skipped.`);
      }
    } catch (e) {
      setActionError(errMsg(e));
    } finally {
      setExporting(false);
    }
  }

  async function verifyIntegrity() {
    setActionError(null);
    const slice = backups.slice(0, VERIFY_CAP);
    if (slice.length === 0) return;
    let ok = 0;
    let failed = 0;
    const failures: string[] = [];
    for (let i = 0; i < slice.length; i += 1) {
      const b = slice[i];
      setVerifyProgress(`Verifying ${i + 1}/${slice.length} — ${b.repo_full_name}`);
      try {
        const r = await backup_verify({ backupId: b.id });
        if (r.ok) ok += 1;
        else {
          failed += 1;
          failures.push(`${b.repo_full_name}: ${r.detail}`);
        }
      } catch (e) {
        failed += 1;
        failures.push(`${b.repo_full_name}: ${errMsg(e)}`);
      }
    }
    setVerifyProgress(null);
    setVerifySummary({ ok, failed, failures });
    reload();
  }

  if (loading && !data) return <PageLoading rows={6} />;

  const maxRepoBytes = info ? Math.max(1, ...info.per_repo.map((r) => r.bytes)) : 1;

  return (
    <div className="space-y-4">
      <ErrorBanner error={error ?? actionError} onRetry={error ? reload : undefined} />

      {/* Root directory */}
      <Card className="p-4">
        <div className="flex flex-wrap items-end gap-3">
          <Field label="Backup root directory" htmlFor="storage-root" className="min-w-[260px] flex-1">
            <Input
              id="storage-root"
              value={root || info?.root || ''}
              onChange={(e) => setRoot(e.target.value)}
              placeholder={info?.root ?? '/path/to/backups'}
              readOnly={!!info && !root}
            />
          </Field>
          <Button onClick={browseRoot} disabled={browsing || !!error}>
            <HardDrive size={14} aria-hidden />
            {browsing ? 'Opening picker…' : 'Change root'}
          </Button>
          <Button onClick={reload} disabled={!!error}>
            Refresh
          </Button>
        </div>
      </Card>

      {!error && !info && !loading && (
        <EmptyState
          icon={HardDrive}
          title="No storage information"
          description="The backend did not return any storage data. Once a backup root exists, usage numbers appear here."
        />
      )}

      {info && (
        <>
          <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 xl:grid-cols-5">
            <StatCard label="Used" value={formatBytes(info.total_bytes)} hint="All backups on disk" />
            <StatCard label="Available" value={formatBytes(info.available_bytes)} hint="Free space on the volume" />
            <StatCard label="Backups" value={info.backup_count} hint="Stored backup records" />
            <StatCard label="Repositories" value={info.repo_count} hint="With at least one backup" />
            <StatCard label="Snapshots" value={info.snapshot_count} hint="Individual snapshots retained" />
          </div>

          <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
            {/* Per-repo usage */}
            <Card>
              <CardHeader
                title="Usage by repository"
                description="How much of the root each repository occupies"
              />
              {info.per_repo.length === 0 ? (
                <div className="px-5 py-6">
                  <EmptyState
                    icon={HardDrive}
                    title="No repository usage yet"
                    description="Usage bars appear once at least one repository has been backed up."
                  />
                </div>
              ) : (
                <ul className="space-y-4 px-5 py-4">
                  {info.per_repo.map((r) => (
                    <li key={r.full_name}>
                      <div className="mb-1.5 flex items-center justify-between gap-3 text-sm">
                        <span className="truncate text-foreground" title={r.full_name}>
                          {r.full_name}
                        </span>
                        <span className="shrink-0 text-xs text-muted">
                          {formatBytes(r.bytes)} · {r.backups}{' '}
                          {r.backups === 1 ? 'backup' : 'backups'}
                        </span>
                      </div>
                      <Progress
                        value={(r.bytes / maxRepoBytes) * 100}
                        label={`Usage of ${r.full_name}`}
                      />
                    </li>
                  ))}
                </ul>
              )}
            </Card>

            {/* Actions */}
            <Card>
              <CardHeader
                title="Maintenance"
                description="Export manifests or re-verify stored backups"
              />
              <div className="space-y-4 px-5 py-4">
                <div className="rounded-lg border border-border bg-surface px-4 py-3">
                  <p className="text-sm font-medium text-foreground">Export manifests</p>
                  <p className="mt-0.5 text-xs text-muted">
                    Downloads a JSON file with the manifests of the {Math.min(EXPORT_CAP, backups.length)}{' '}
                    most recent backups (up to {EXPORT_CAP}).
                  </p>
                  <Button
                    className="mt-3"
                    onClick={exportManifests}
                    disabled={exporting || backups.length === 0}
                  >
                    <Download size={14} aria-hidden />
                    {exporting ? 'Collecting manifests…' : 'Export manifests'}
                  </Button>
                </div>

                <div className="rounded-lg border border-border bg-surface px-4 py-3">
                  <p className="text-sm font-medium text-foreground">Verify integrity</p>
                  <p className="mt-0.5 text-xs text-muted">
                    Recomputes checksums for the {Math.min(VERIFY_CAP, backups.length)} most
                    recent backups (up to {VERIFY_CAP}) and reports failures.
                  </p>
                  {verifyProgress ? (
                    <p className="mt-3 flex items-center gap-2 text-xs text-secondary">
                      <SearchCheck size={13} aria-hidden />
                      {verifyProgress}
                    </p>
                  ) : (
                    <Button
                      className="mt-3"
                      onClick={verifyIntegrity}
                      disabled={backups.length === 0}
                    >
                      <SearchCheck size={14} aria-hidden />
                      Verify integrity
                    </Button>
                  )}
                </div>

                <p className="text-xs text-muted">
                  Root: <span className="font-mono">{info.root}</span>
                </p>
              </div>
            </Card>
          </div>

          {/* Backups table */}
          {backups.length === 0 ? (
            <EmptyState
              icon={HardDrive}
              title="No backups stored"
              description="Run a backup first — every stored backup then appears here with a delete button."
            />
          ) : (
            <Table>
              <thead>
                <tr>
                  <TH>Repository</TH>
                  <TH>Mode</TH>
                  <TH>Size</TH>
                  <TH>Files</TH>
                  <TH>Created</TH>
                  <TH>Status</TH>
                  <TH className="text-right">Actions</TH>
                </tr>
              </thead>
              <tbody>
                {backups.map((b) => (
                  <TR key={b.id}>
                    <TD>
                      <span className="block font-medium text-foreground">{b.repo_full_name}</span>
                      <span className="block truncate text-xs text-muted" title={b.path}>
                        {b.path}
                      </span>
                    </TD>
                    <TD className="whitespace-nowrap">
                      {b.mode === 'full' ? 'Complete' : 'Selective'}
                    </TD>
                    <TD className="whitespace-nowrap text-xs">{formatBytes(b.size_bytes)}</TD>
                    <TD className="text-xs">{b.file_count}</TD>
                    <TD className="whitespace-nowrap text-xs">{formatDate(b.created_at)}</TD>
                    <TD>
                      <div className="flex items-center gap-1.5">
                        <Badge tone={b.status === 'failed' ? 'error' : 'success'}>{b.status}</Badge>
                        {b.verified && <Badge tone="yellow">Verified</Badge>}
                      </div>
                    </TD>
                    <TD>
                      <div className="flex justify-end">
                        <Button
                          size="sm"
                          variant="ghost"
                          className="text-error hover:text-error"
                          onClick={() => setDeleteTarget(b)}
                          aria-label={`Delete backup of ${b.repo_full_name}`}
                        >
                          <Trash2 size={13} aria-hidden />
                          Delete
                        </Button>
                      </div>
                    </TD>
                  </TR>
                ))}
              </tbody>
            </Table>
          )}
        </>
      )}

      {/* Verify summary */}
      <Dialog
        open={!!verifySummary}
        onClose={() => setVerifySummary(null)}
        title="Integrity verification"
        width="max-w-xl"
        footer={<Button onClick={() => setVerifySummary(null)}>Close</Button>}
      >
        {verifySummary && (
          <div className="space-y-3">
            <div className="flex items-center gap-3">
              <CheckCircle2 size={18} className="text-success" aria-hidden />
              <p className="text-sm text-foreground">
                <span className="font-semibold">{verifySummary.ok}</span> backup(s) verified
                successfully, <span className="font-semibold">{verifySummary.failed}</span>{' '}
                failed.
              </p>
            </div>
            {verifySummary.failures.length > 0 && (
              <ul className="list-inside list-disc space-y-1 text-sm text-error">
                {verifySummary.failures.map((f) => (
                  <li key={f}>{f}</li>
                ))}
              </ul>
            )}
          </div>
        )}
      </Dialog>

      {/* Delete confirmation */}
      <Dialog
        open={!!deleteTarget}
        onClose={() => setDeleteTarget(null)}
        title="Delete backup"
        footer={
          <>
            <Button onClick={() => setDeleteTarget(null)} disabled={deleting}>
              Cancel
            </Button>
            <Button variant="danger" onClick={confirmDelete} disabled={deleting}>
              {deleting ? 'Deleting…' : 'Delete backup'}
            </Button>
          </>
        }
      >
        <p>
          Delete the backup of{' '}
          <span className="font-medium text-foreground">{deleteTarget?.repo_full_name}</span> from{' '}
          {formatDate(deleteTarget?.created_at)}? Payload and manifest are removed from disk. This
          cannot be undone.
        </p>
      </Dialog>
    </div>
  );
}
