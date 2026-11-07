import { FileJson, HardDrive, RotateCcw } from 'lucide-react';
import { useState } from 'react';
import { backup_history, backup_manifest, pick_directory, restore_start } from '../api/client';
import type { BackupRecord, ConflictMode } from '../api/types';
import {
  Badge,
  Button,
  Card,
  CardHeader,
  Checkbox,
  EmptyState,
  ErrorBanner,
  Field,
  Input,
  LoadingRow,
  Notice,
  PageLoading,
} from '../components/ui';
import { errMsg, formatBytes, formatDate } from '../lib/format';
import { useAsync } from '../lib/hooks';
import { useNav } from '../store/nav';

const CONFLICT_OPTIONS: {
  value: ConflictMode;
  label: string;
  description: string;
}[] = [
  {
    value: 'new_folder',
    label: 'New folder',
    description: 'Restore into a fresh folder — nothing existing is touched.',
  },
  {
    value: 'merge',
    label: 'Merge',
    description: 'Write into the destination, overwriting only files that differ.',
  },
  {
    value: 'replace',
    label: 'Replace (with warning)',
    description: 'Clear the destination first, then restore. Existing data is lost.',
  },
];

function ManifestPanel({ backupId }: { backupId: string }) {
  const { data, error, loading, reload } = useAsync(
    () => backup_manifest({ backupId }),
    [backupId],
  );

  if (loading) return <LoadingRow text="Loading manifest…" />;
  if (error) return <ErrorBanner error={error} onRetry={reload} />;
  if (!data) return null;

  const m = data;
  return (
    <div className="space-y-4">
      <dl className="grid grid-cols-2 gap-x-4 gap-y-3 text-sm">
        <div>
          <dt className="text-xs text-muted">Repository</dt>
          <dd className="font-medium text-foreground">{m.repository.full_name}</dd>
        </div>
        <div>
          <dt className="text-xs text-muted">Created</dt>
          <dd className="text-foreground">{formatDate(m.created_at)}</dd>
        </div>
        <div>
          <dt className="text-xs text-muted">Mode / format</dt>
          <dd className="text-foreground">
            {m.mode === 'full' ? 'Complete' : 'Selective'} · {m.format}
          </dd>
        </div>
        <div>
          <dt className="text-xs text-muted">Size</dt>
          <dd className="text-foreground">
            {formatBytes(m.size_bytes)} · {m.file_count} files
          </dd>
        </div>
        <div>
          <dt className="text-xs text-muted">Head commit</dt>
          <dd className="break-all font-mono text-xs text-foreground">{m.head_sha ?? '—'}</dd>
        </div>
        <div>
          <dt className="text-xs text-muted">References</dt>
          <dd className="text-foreground">{m.refs.length}</dd>
        </div>
        <div>
          <dt className="text-xs text-muted">Payload</dt>
          <dd className="break-all font-mono text-xs text-foreground">{m.payload_path}</dd>
        </div>
        <div>
          <dt className="text-xs text-muted">Verification</dt>
          <dd className="text-foreground">
            <Badge tone={m.verification.status === 'ok' ? 'success' : 'warning'}>
              {m.verification.status}
            </Badge>
          </dd>
        </div>
      </dl>

      <div className="rounded-lg border border-border bg-surface px-4 py-3">
        <p className="text-xs font-semibold uppercase tracking-wide text-muted">
          Limitations of this backup (verbatim from manifest)
        </p>
        {m.limitations.length === 0 ? (
          <p className="mt-2 text-sm text-muted">No limitations recorded.</p>
        ) : (
          <ul className="mt-2 list-inside list-disc space-y-1 text-sm text-secondary">
            {m.limitations.map((l, i) => (
              <li key={i}>{l}</li>
            ))}
          </ul>
        )}
      </div>
    </div>
  );
}

export default function Restore() {
  const { navigate } = useNav();
  const { data: backups, error, loading, reload } = useAsync(
    () => backup_history({ limit: 100 }),
    [],
  );

  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [destination, setDestination] = useState('');
  const [conflict, setConflict] = useState<ConflictMode>('new_folder');
  const [replaceAck, setReplaceAck] = useState(false);
  const [starting, setStarting] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);

  const chosen: BackupRecord | undefined = backups?.find((b) => b.id === selectedId);

  async function browse() {
    setActionError(null);
    try {
      const path = await pick_directory();
      if (path) setDestination(path);
    } catch (e) {
      setActionError(errMsg(e));
    }
  }

  async function start() {
    setActionError(null);
    if (!chosen) {
      setActionError('Choose a backup to restore first.');
      return;
    }
    if (!destination.trim()) {
      setActionError('Choose a destination folder for the restore.');
      return;
    }
    if (conflict === 'replace' && !replaceAck) {
      setActionError('Confirm the replace warning before restoring.');
      return;
    }
    setStarting(true);
    try {
      await restore_start({
        req: {
          backup_id: chosen.id,
          destination: destination.trim(),
          conflict_mode: conflict,
        },
      });
      navigate('tasks');
    } catch (e) {
      setActionError(errMsg(e));
    } finally {
      setStarting(false);
    }
  }

  if (loading && !backups) return <PageLoading rows={6} />;

  const canStart = !!chosen && destination.trim().length > 0 && (conflict !== 'replace' || replaceAck);

  return (
    <div className="space-y-4">
      <ErrorBanner error={error ?? actionError} onRetry={error ? reload : undefined} />

      {!error && backups && backups.length === 0 && (
        <EmptyState
          icon={RotateCcw}
          title="No backups to restore"
          description="Once you run a backup it will be listed here with its manifest, so you can inspect exactly what will be restored."
          action={
            <Button variant="primary" onClick={() => navigate('backup')}>
              Go to Backup
            </Button>
          }
        />
      )}

      {!error && backups && backups.length > 0 && (
        <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
          {/* Backup list */}
          <Card>
            <CardHeader
              title="Choose a backup"
              description={`${backups.length} restore points available`}
            />
            <ul className="max-h-[560px] divide-y divide-[#EAEAEA] overflow-y-auto">
              {backups.map((b) => {
                const active = b.id === selectedId;
                return (
                  <li key={b.id}>
                    <button
                      type="button"
                      onClick={() => setSelectedId(b.id)}
                      aria-pressed={active}
                      className={`flex w-full items-center gap-3 px-5 py-3 text-left transition-colors duration-150 ${
                        active ? 'bg-pale' : 'hover:bg-surface'
                      }`}
                    >
                      <span
                        className={`h-9 w-1 shrink-0 rounded-full ${active ? 'bg-primary' : 'bg-transparent'}`}
                        aria-hidden
                      />
                      <span className="min-w-0 flex-1">
                        <span className="block truncate text-sm font-medium text-foreground">
                          {b.repo_full_name}
                        </span>
                        <span className="block truncate text-xs text-muted">
                          {b.mode === 'full' ? 'Complete' : 'Selective'}
                          {b.format ? ` · ${b.format}` : ''} · {formatBytes(b.size_bytes)} ·{' '}
                          {formatDate(b.created_at)}
                        </span>
                      </span>
                      {b.verified && <Badge tone="yellow">Verified</Badge>}
                    </button>
                  </li>
                );
              })}
            </ul>
          </Card>

          {/* Details + restore form */}
          <div className="space-y-4">
            <Card>
              <CardHeader
                title="Manifest details"
                description={
                  chosen
                    ? `Manifest for ${chosen.repo_full_name}`
                    : 'Select a backup on the left to inspect its manifest'
                }
                actions={
                  chosen && (
                    <Badge tone="neutral">
                      <FileJson size={11} aria-hidden />
                      {chosen.manifest_path ? 'Manifest on disk' : 'No manifest path'}
                    </Badge>
                  )
                }
              />
              <div className="px-5 py-4">
                {!chosen ? (
                  <p className="py-6 text-center text-sm text-muted">
                    No backup selected yet.
                  </p>
                ) : (
                  <ManifestPanel backupId={chosen.id} />
                )}
              </div>
            </Card>

            <Card>
              <CardHeader title="Restore settings" />
              <div className="space-y-4 px-5 py-4">
                <Field label="Destination folder" htmlFor="restore-destination">
                  <div className="flex gap-2">
                    <Input
                      id="restore-destination"
                      value={destination}
                      onChange={(e) => setDestination(e.target.value)}
                      placeholder="/path/to/restore"
                    />
                    <Button onClick={browse} aria-label="Browse for restore destination">
                      <HardDrive size={14} aria-hidden />
                      Browse
                    </Button>
                  </div>
                </Field>

                <fieldset>
                  <legend className="mb-1.5 text-sm font-medium text-foreground">
                    If the destination already has files
                  </legend>
                  <div className="space-y-2">
                    {CONFLICT_OPTIONS.map((opt) => (
                      <label
                        key={opt.value}
                        className={`flex cursor-pointer items-start gap-3 rounded-lg border px-4 py-3 transition-colors duration-150 ${
                          conflict === opt.value
                            ? 'border-[#F3E3A0] bg-pale'
                            : 'border-border hover:bg-surface'
                        }`}
                      >
                        <input
                          type="radio"
                          name="conflict-mode"
                          value={opt.value}
                          checked={conflict === opt.value}
                          onChange={() => {
                            setConflict(opt.value);
                            if (opt.value !== 'replace') setReplaceAck(false);
                          }}
                          className="mt-0.5 h-4 w-4 accent-primary"
                        />
                        <span>
                          <span className="block text-sm font-medium text-foreground">
                            {opt.label}
                          </span>
                          <span className="block text-xs text-muted">{opt.description}</span>
                        </span>
                      </label>
                    ))}
                  </div>
                </fieldset>

                {conflict === 'replace' && (
                  <Notice tone="warning" title="This will overwrite existing files">
                    <p className="text-[#B91C1C]">
                      Replace mode clears the destination before restoring. Any files that are
                      not part of this backup will be permanently removed.
                    </p>
                    <label className="mt-3 flex cursor-pointer items-start gap-2.5">
                      <Checkbox
                        checked={replaceAck}
                        onChange={(e) => setReplaceAck(e.target.checked)}
                        className="mt-0.5"
                      />
                      <span className="font-medium text-[#B91C1C]">
                        I understand this may permanently overwrite my data
                      </span>
                    </label>
                  </Notice>
                )}

                <div className="flex items-center justify-between gap-3 border-t border-border pt-4">
                  <p className="text-xs text-muted">
                    {chosen ? `Ready to restore ${chosen.repo_full_name}` : 'Select a backup to continue'}
                  </p>
                  <Button variant="primary" size="lg" onClick={start} disabled={!canStart || starting}>
                    <RotateCcw size={15} aria-hidden />
                    {starting ? 'Starting…' : 'Start restore'}
                  </Button>
                </div>
              </div>
            </Card>
          </div>
        </div>
      )}
    </div>
  );
}
