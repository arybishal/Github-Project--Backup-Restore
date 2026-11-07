import { CheckCircle2, Eye, FileJson, Search, ShieldCheck, Trash2 } from 'lucide-react';
import { useEffect, useMemo, useState } from 'react';
import { backup_delete, backup_history, backup_manifest, backup_verify } from '../api/client';
import type { BackupRecord } from '../api/types';
import {
  Badge,
  Button,
  Card,
  Dialog,
  EmptyState,
  ErrorBanner,
  Input,
  PageLoading,
  Select,
  Table,
  TD,
  TH,
  TR,
} from '../components/ui';
import { errMsg, formatBytes, formatDate } from '../lib/format';
import { useAsync } from '../lib/hooks';
import { useNav } from '../store/nav';

function statusTone(status: string): 'success' | 'error' | 'warning' | 'neutral' {
  const s = status.toLowerCase();
  if (['succeeded', 'success', 'ok', 'verified'].includes(s)) return 'success';
  if (['failed', 'error'].includes(s)) return 'error';
  if (['running', 'pending', 'queued'].includes(s)) return 'warning';
  return 'neutral';
}

function ManifestDialog({ backupId, onClose }: { backupId: string; onClose: () => void }) {
  const { data, error, loading, reload } = useAsync(
    () => backup_manifest({ backupId }),
    [backupId],
  );

  return (
    <Dialog
      open
      onClose={onClose}
      title="Backup manifest"
      width="max-w-3xl"
      footer={
        <Button onClick={onClose} variant="outline">
          Close
        </Button>
      }
    >
      {loading && <p className="py-4 text-center text-sm text-muted">Loading manifest…</p>}
      {error && <ErrorBanner error={error} onRetry={reload} />}
      {data && (
        <div className="space-y-4">
          <div className="rounded-lg border border-border bg-surface px-4 py-3">
            <p className="text-xs font-semibold uppercase tracking-wide text-muted">
              Limitations (verbatim)
            </p>
            {data.limitations.length === 0 ? (
              <p className="mt-2 text-sm text-muted">No limitations recorded.</p>
            ) : (
              <ul className="mt-2 list-inside list-disc space-y-1 text-sm text-secondary">
                {data.limitations.map((l, i) => (
                  <li key={i}>{l}</li>
                ))}
              </ul>
            )}
          </div>
          <pre className="max-h-[420px] overflow-auto rounded-lg border border-border bg-[#FAFAFA] p-4 text-[11px] leading-relaxed text-foreground">
            {JSON.stringify(data, null, 2)}
          </pre>
        </div>
      )}
    </Dialog>
  );
}

export default function BackupHistory() {
  const { navigate } = useNav();
  const { data: records, error, loading, reload } = useAsync(
    () => backup_history({ limit: 200 }),
    [],
  );

  const [search, setSearch] = useState('');
  const [mode, setMode] = useState<'all' | 'full' | 'selective'>('all');
  const [status, setStatus] = useState('all');
  const [verified, setVerified] = useState<'all' | 'yes' | 'no'>('all');

  const [verifyTarget, setVerifyTarget] = useState<BackupRecord | null>(null);
  const [verifyResult, setVerifyResult] = useState<string | null>(null);
  const [verifyError, setVerifyError] = useState<string | null>(null);
  const [verifying, setVerifying] = useState(false);

  const [manifestId, setManifestId] = useState<string | null>(null);
  const [deleteTarget, setDeleteTarget] = useState<BackupRecord | null>(null);
  const [deleting, setDeleting] = useState(false);
  const [deleteError, setDeleteError] = useState<string | null>(null);

  useEffect(() => {
    if (!verifyTarget) return;
    let alive = true;
    setVerifying(true);
    setVerifyResult(null);
    setVerifyError(null);
    backup_verify({ backupId: verifyTarget.id })
      .then((r) => {
        if (!alive) return;
        setVerifyResult(
          `${r.ok ? 'Verified OK' : 'Verification FAILED'} · ${r.checked_files} files checked · ${r.detail}`,
        );
      })
      .catch((e: unknown) => {
        if (alive) setVerifyError(errMsg(e));
      })
      .finally(() => {
        if (alive) setVerifying(false);
      });
    return () => {
      alive = false;
    };
  }, [verifyTarget]);

  const filtered = useMemo(() => {
    const q = search.trim().toLowerCase();
    return (records ?? []).filter((r) => {
      if (q && !r.repo_full_name.toLowerCase().includes(q)) return false;
      if (mode !== 'all' && r.mode !== mode) return false;
      if (status !== 'all' && r.status !== status) return false;
      if (verified === 'yes' && !r.verified) return false;
      if (verified === 'no' && r.verified) return false;
      return true;
    });
  }, [records, search, mode, status, verified]);

  async function confirmDelete() {
    if (!deleteTarget) return;
    setDeleting(true);
    setDeleteError(null);
    try {
      await backup_delete({ backupId: deleteTarget.id });
      setDeleteTarget(null);
      reload();
    } catch (e) {
      setDeleteError(errMsg(e));
    } finally {
      setDeleting(false);
    }
  }

  if (loading && !records) return <PageLoading rows={8} />;

  const statuses = [...new Set((records ?? []).map((r) => r.status))].sort();

  return (
    <div className="space-y-4">
      <ErrorBanner error={error ?? deleteError} onRetry={error ? reload : undefined} />

      {!error && records && records.length === 0 && (
        <EmptyState
          icon={FileJson}
          title="No backup history yet"
          description="Every backup you run is recorded here with its verification status, so you can audit and re-verify them later."
          action={
            <Button variant="primary" onClick={() => navigate('backup')}>
              Create the first backup
            </Button>
          }
        />
      )}

      {!error && records && records.length > 0 && (
        <>
          <Card className="p-4">
            <div className="flex flex-wrap items-end gap-2">
              <div className="relative min-w-[200px] flex-1">
                <Search
                  size={14}
                  className="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-muted"
                  aria-hidden
                />
                <Input
                  value={search}
                  onChange={(e) => setSearch(e.target.value)}
                  placeholder="Filter by repository…"
                  aria-label="Filter backups by repository"
                  className="pl-8"
                />
              </div>
              <label className="flex flex-col gap-1 text-xs font-medium text-muted">
                Mode
                <Select
                  value={mode}
                  onChange={(e) => setMode(e.target.value as 'all' | 'full' | 'selective')}
                  aria-label="Filter by mode"
                  className="w-36"
                >
                  <option value="all">All modes</option>
                  <option value="full">Complete</option>
                  <option value="selective">Selective</option>
                </Select>
              </label>
              <label className="flex flex-col gap-1 text-xs font-medium text-muted">
                Status
                <Select
                  value={status}
                  onChange={(e) => setStatus(e.target.value)}
                  aria-label="Filter by status"
                  className="w-36"
                >
                  <option value="all">All statuses</option>
                  {statuses.map((s) => (
                    <option key={s} value={s}>
                      {s}
                    </option>
                  ))}
                </Select>
              </label>
              <label className="flex flex-col gap-1 text-xs font-medium text-muted">
                Verified
                <Select
                  value={verified}
                  onChange={(e) => setVerified(e.target.value as 'all' | 'yes' | 'no')}
                  aria-label="Filter by verification"
                  className="w-36"
                >
                  <option value="all">Any</option>
                  <option value="yes">Verified</option>
                  <option value="no">Unverified</option>
                </Select>
              </label>
            </div>
          </Card>

          {filtered.length === 0 ? (
            <EmptyState
              icon={Search}
              title="No backups match your filters"
              description="Adjust the search term or filters to find a specific backup."
              action={
                <Button
                  onClick={() => {
                    setSearch('');
                    setMode('all');
                    setStatus('all');
                    setVerified('all');
                  }}
                >
                  Clear filters
                </Button>
              }
            />
          ) : (
            <Table>
              <thead>
                <tr>
                  <TH>Repository</TH>
                  <TH>Mode</TH>
                  <TH>Format</TH>
                  <TH>Size</TH>
                  <TH>Date</TH>
                  <TH>Status</TH>
                  <TH>Verified</TH>
                  <TH className="text-right">Actions</TH>
                </tr>
              </thead>
              <tbody>
                {filtered.map((r) => (
                  <TR key={r.id}>
                    <TD>
                      <span className="block font-medium text-foreground">{r.repo_full_name}</span>
                      <span className="block text-xs text-muted">{r.file_count} files</span>
                    </TD>
                    <TD className="whitespace-nowrap">
                      {r.mode === 'full' ? 'Complete' : 'Selective'}
                    </TD>
                    <TD className="whitespace-nowrap text-xs">{r.format ?? '—'}</TD>
                    <TD className="whitespace-nowrap text-xs">{formatBytes(r.size_bytes)}</TD>
                    <TD className="whitespace-nowrap text-xs">{formatDate(r.created_at)}</TD>
                    <TD>
                      <Badge tone={statusTone(r.status)}>{r.status}</Badge>
                    </TD>
                    <TD>
                      {r.verified ? (
                        <Badge tone="yellow">
                          <CheckCircle2 size={10} aria-hidden />
                          Yes
                        </Badge>
                      ) : (
                        <span className="text-xs text-muted">No</span>
                      )}
                    </TD>
                    <TD>
                      <div className="flex justify-end gap-1.5">
                        <Button
                          size="sm"
                          variant="ghost"
                          onClick={() => setVerifyTarget(r)}
                          aria-label={`Verify backup of ${r.repo_full_name}`}
                        >
                          <ShieldCheck size={13} aria-hidden />
                          Verify
                        </Button>
                        <Button
                          size="sm"
                          variant="ghost"
                          onClick={() => setManifestId(r.id)}
                          aria-label={`View manifest for ${r.repo_full_name}`}
                        >
                          <Eye size={13} aria-hidden />
                          Manifest
                        </Button>
                        <Button
                          size="sm"
                          variant="ghost"
                          className="text-error hover:text-error"
                          onClick={() => setDeleteTarget(r)}
                          aria-label={`Delete backup of ${r.repo_full_name}`}
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

      {/* Verify result */}
      <Dialog
        open={!!verifyTarget}
        onClose={() => {
          setVerifyTarget(null);
          setVerifyResult(null);
          setVerifyError(null);
        }}
        title={`Verify backup${verifyTarget ? ` — ${verifyTarget.repo_full_name}` : ''}`}
        footer={
          <Button
            onClick={() => {
              setVerifyTarget(null);
              setVerifyResult(null);
              setVerifyError(null);
            }}
          >
            Close
          </Button>
        }
      >
        {verifying && <p className="py-2 text-sm text-muted">Verifying checksums…</p>}
        {verifyError && <ErrorBanner error={verifyError} />}
        {verifyResult && (
          <p
            className={`text-sm ${verifyResult.startsWith('Verified OK') ? 'text-success' : 'text-error'}`}
          >
            {verifyResult}
          </p>
        )}
      </Dialog>

      {/* Manifest viewer */}
      {manifestId && <ManifestDialog backupId={manifestId} onClose={() => setManifestId(null)} />}

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
          <span className="font-medium text-foreground">{deleteTarget?.repo_full_name}</span>{' '}
          from {formatDate(deleteTarget?.created_at)}? The payload and manifest on disk will be
          removed. This cannot be undone.
        </p>
      </Dialog>
    </div>
  );
}
