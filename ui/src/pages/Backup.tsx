import { AlertTriangle, FolderOpen, Play, Save } from 'lucide-react';
import { useEffect, useMemo, useState } from 'react';
import { backup_start, pick_directory, repos_list, settings_get, tasks_list } from '../api/client';
import type { BackupFormat, BackupRequest, Repo, TaskDto } from '../api/types';
import {
  Badge,
  Button,
  Card,
  CardHeader,
  Checkbox,
  ErrorBanner,
  Field,
  Input,
  Notice,
  PageLoading,
  Progress,
  Segmented,
  Select,
  Toggle,
  cn,
} from '../components/ui';
import { errMsg } from '../lib/format';
import { useAsync } from '../lib/hooks';
import { useNav } from '../store/nav';
import { setSelectedRepos, useSelectedRepos } from '../store/selection';

type BackupMode2 = BackupRequest['mode'];

const SENSITIVE_TESTS: [string, RegExp][] = [
  ['.env file', /(^|[/\\])\.env([./\\]|$)/i],
  ['*.pem key', /\.pem$/i],
  ['id_rsa private key', /(^|[/\\])id_rsa$/i],
  ['credentials file', /(^|[/\\])credentials([^/\\]*)$/i],
];

function sensitiveHits(paths: string[]): { kind: string; path: string }[] {
  const hits: { kind: string; path: string }[] = [];
  for (const p of paths) {
    if (!p) continue;
    for (const [kind, re] of SENSITIVE_TESTS) {
      if (re.test(p)) hits.push({ kind, path: p });
    }
  }
  return hits;
}

function splitPatterns(text: string): string[] {
  return text
    .split(/[\n,]/)
    .map((s) => s.trim())
    .filter(Boolean);
}

export default function Backup() {
  const { navigate } = useNav();
  const { data, error, loading } = useAsync(
    () => Promise.all([repos_list({ filter: {} }), settings_get()]),
    [],
  );
  const selected = useSelectedRepos();

  const [prefilled, setPrefilled] = useState(false);
  const [mode, setMode] = useState<BackupMode2>('full');
  const [destination, setDestination] = useState('');
  const [format, setFormat] = useState<BackupFormat>('folder');
  const [retention, setRetention] = useState('');
  const [includeLfs, setIncludeLfs] = useState(true);
  const [allRepos, setAllRepos] = useState(false);
  const [checkoutRoot, setCheckoutRoot] = useState('');
  const [includePatterns, setIncludePatterns] = useState('');
  const [excludePatterns, setExcludePatterns] = useState('');
  const [sensitiveAck, setSensitiveAck] = useState(false);
  const [includeHidden, setIncludeHidden] = useState(false);
  const [includeIgnored, setIncludeIgnored] = useState(false);
  const [starting, setStarting] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);
  const [startedId, setStartedId] = useState<string | null>(null);
  const [startedTask, setStartedTask] = useState<TaskDto | null>(null);

  const repos: Repo[] = data?.[0] ?? [];

  // backup_start returns the task id immediately; engine failures (e.g. the
  // secret-file hard gate) are recorded on the task, so watch it here and put
  // any error into the inline banner instead of swallowing it.
  useEffect(() => {
    if (!startedId) return;
    let alive = true;
    let timer: number | undefined;
    const poll = async () => {
      try {
        const list = await tasks_list();
        const t = list.find((x) => x.id === startedId) ?? null;
        if (!alive) return;
        setStartedTask(t);
        if (t && ['succeeded', 'failed', 'cancelled'].includes(t.status)) {
          if (t.status === 'failed') {
            setActionError(t.error ?? 'The backup job failed.');
          }
          return; // terminal — stop polling
        }
      } catch (e) {
        if (alive) setActionError(errMsg(e));
        return;
      }
      timer = window.setTimeout(poll, 1000);
    };
    void poll();
    return () => {
      alive = false;
      if (timer !== undefined) window.clearTimeout(timer);
    };
  }, [startedId]);

  useEffect(() => {
    if (data && !prefilled) {
      const settings = data[1];
      setDestination(settings.backup_root);
      setFormat(settings.default_format);
      setMode(settings.default_mode);
      setRetention('');
      setPrefilled(true);
    }
  }, [data, prefilled]);

  async function browse(onPick: (path: string) => void) {
    setActionError(null);
    try {
      const path = await pick_directory();
      if (path) onPick(path);
    } catch (e) {
      setActionError(errMsg(e));
    }
  }

  const hits = useMemo(
    () =>
      mode === 'selective'
        ? sensitiveHits([checkoutRoot, ...splitPatterns(includePatterns)])
        : [],
    [mode, checkoutRoot, includePatterns],
  );

  function toggleRepo(name: string) {
    if (selected.includes(name)) {
      setSelectedRepos(selected.filter((n) => n !== name));
    } else {
      setSelectedRepos([...selected, name]);
    }
  }

  async function start() {
    setActionError(null);
    if (mode === 'full' && !allRepos && selected.length === 0) {
      setActionError('Select at least one repository, or enable "All repositories".');
      return;
    }
    if (mode === 'selective' && !checkoutRoot.trim()) {
      setActionError('Choose the local checkout folder for the selective backup.');
      return;
    }
    if (mode === 'selective' && hits.length > 0 && !sensitiveAck) {
      setActionError('Confirm the sensitive file warning before starting the backup.');
      return;
    }
    setStarting(true);
    try {
      const req: BackupRequest = {
        mode,
        format,
        destination: destination.trim() || null,
        repos: mode === 'full' && allRepos ? [] : selected,
        all: mode === 'full' && allRepos,
        include_lfs: includeLfs,
        retention: retention.trim() === '' ? undefined : Number(retention),
        // Hard gate: the engine refuses secret-looking files unless this is true.
        acknowledge_secrets: sensitiveAck,
        ...(mode === 'selective'
          ? {
              checkout_root: checkoutRoot.trim() || null,
              include_patterns: splitPatterns(includePatterns),
              exclude_patterns: splitPatterns(excludePatterns),
              hidden: includeHidden,
              ignored: includeIgnored,
            }
          : {}),
      };
      const id = await backup_start({ req });
      setStartedId(id);
      setStartedTask(null);
    } catch (e) {
      setActionError(errMsg(e));
    } finally {
      setStarting(false);
    }
  }

  if (loading && !data) return <PageLoading rows={5} />;

  const jobTerminal =
    !!startedTask && ['succeeded', 'failed', 'cancelled'].includes(startedTask.status);
  const jobTone =
    startedTask?.status === 'succeeded'
      ? 'success'
      : startedTask?.status === 'failed'
        ? 'error'
        : 'neutral';

  return (
    <div className="space-y-4">
      <ErrorBanner error={error ?? actionError} />

      {/* Live status of the job we just started — engine rejections (e.g. the
          secret-file hard gate) surface here via the inline banner above. */}
      {startedId && (
        <Card className="p-4">
          <div className="flex flex-wrap items-center justify-between gap-3">
            <div className="min-w-0 flex-1">
              <p className="flex items-center gap-2 text-sm font-semibold text-foreground">
                Backup job started
                {startedTask && <Badge tone={jobTone}>{startedTask.status}</Badge>}
              </p>
              <p className="mt-0.5 truncate text-xs text-muted">
                {startedTask?.detail ?? 'Waiting for the first progress update…'}
              </p>
            </div>
            <div className="flex items-center gap-3">
              {!jobTerminal && (
                <span className="text-sm font-semibold text-foreground">
                  {Math.round(startedTask?.progress ?? 0)}%
                </span>
              )}
              <Button size="sm" onClick={() => navigate('tasks')}>
                Open Task Center
              </Button>
            </div>
          </div>
          {!jobTerminal && (
            <div className="mt-3">
              <Progress value={startedTask?.progress ?? 0} label="Backup progress" />
            </div>
          )}
          {startedTask?.status === 'succeeded' && (
            <p className="mt-2 text-xs text-success">
              Backup finished successfully — review it in Backup History.
            </p>
          )}
        </Card>
      )}

      <Card>
        <CardHeader
          title="Backup options"
          description="Choose what to back up and where it goes"
          actions={
            <Segmented
              ariaLabel="Backup mode"
              value={mode}
              onChange={setMode}
              options={[
                { value: 'full', label: 'Complete Git Backup' },
                { value: 'selective', label: 'Selective Backup' },
              ]}
            />
          }
        />
        <div className="grid grid-cols-1 gap-4 px-5 py-4 md:grid-cols-2">
          <Field
            label="Destination folder"
            htmlFor="backup-destination"
            hint="Where backups are written. Leave empty to use the default backup root from Settings."
          >
            <div className="flex gap-2">
              <Input
                id="backup-destination"
                value={destination}
                onChange={(e) => setDestination(e.target.value)}
                placeholder="/path/to/backups"
              />
              <Button onClick={() => browse(setDestination)} aria-label="Browse for destination folder">
                <FolderOpen size={14} aria-hidden />
                Browse
              </Button>
            </div>
          </Field>

          <div className="grid grid-cols-2 gap-4">
            <Field label="Format" htmlFor="backup-format">
              <Select
                id="backup-format"
                value={format}
                onChange={(e) => setFormat(e.target.value as BackupFormat)}
              >
                <option value="folder">folder</option>
                <option value="zip">zip</option>
                <option value="tar">tar</option>
                <option value="tar.zst">tar.zst</option>
              </Select>
            </Field>
            <Field
              label="Retention"
              htmlFor="backup-retention"
              hint="Snapshots to keep per repo (0 or empty = keep all)."
            >
              <Input
                id="backup-retention"
                type="number"
                min={0}
                value={retention}
                onChange={(e) => setRetention(e.target.value)}
                placeholder="keep all"
              />
            </Field>
          </div>

          <div className="flex items-center justify-between rounded-lg border border-border bg-surface px-4 py-3 md:col-span-2">
            <div>
              <p className="text-sm font-medium text-foreground">Include Git LFS objects</p>
              <p className="text-xs text-muted">
                Fetch and store LFS payloads when git-lfs is installed.
              </p>
            </div>
            <Toggle checked={includeLfs} onChange={setIncludeLfs} label="Include Git LFS objects" />
          </div>
        </div>
      </Card>

      {mode === 'full' ? (
        <Card>
          <CardHeader
            title="Repositories"
            description={
              allRepos
                ? 'Every repository will be backed up, including ones added later.'
                : 'Pick the repositories to include. The selection you made on the Repositories page is preloaded.'
            }
            actions={
              <div className="flex items-center gap-2">
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={() => setSelectedRepos(repos.map((r) => r.full_name))}
                  disabled={repos.length === 0}
                >
                  Select all
                </Button>
                <Button size="sm" variant="ghost" onClick={() => setSelectedRepos([])}>
                  Clear
                </Button>
              </div>
            }
          />
          <div className="space-y-3 px-5 py-4">
            <label className="flex cursor-pointer items-start gap-2.5 rounded-lg border border-border bg-surface px-4 py-3">
              <Checkbox
                checked={allRepos}
                onChange={(e) => setAllRepos(e.target.checked)}
                className="mt-0.5"
              />
              <span>
                <span className="block text-sm font-medium text-foreground">All repositories</span>
                <span className="block text-xs text-muted">
                  Back up everything, including future repositories.
                </span>
              </span>
            </label>

            <div
              className={
                allRepos ? 'max-h-72 overflow-y-auto opacity-50' : 'max-h-72 overflow-y-auto'
              }
              aria-disabled={allRepos}
            >
              {repos.length === 0 ? (
                <p className="py-6 text-center text-sm text-muted">
                  No repositories loaded. Sync them from the Repositories page first.
                </p>
              ) : (
                <ul className="divide-y divide-[#EAEAEA] rounded-lg border border-border">
                  {repos.map((r) => (
                    <li key={r.github_id}>
                      <label
                        className={cn(
                          'flex cursor-pointer items-center gap-3 px-4 py-2.5 transition-colors duration-150',
                          selected.includes(r.full_name) ? 'bg-pale' : 'hover:bg-surface',
                          allRepos && 'pointer-events-none',
                        )}
                      >
                        <Checkbox
                          checked={selected.includes(r.full_name)}
                          onChange={() => toggleRepo(r.full_name)}
                          disabled={allRepos}
                          aria-label={`Include ${r.full_name}`}
                        />
                        <span className="min-w-0 flex-1">
                          <span className="block truncate text-sm text-foreground">
                            {r.full_name}
                          </span>
                        </span>
                        <Badge tone={r.is_private ? 'dark' : 'neutral'}>
                          {r.is_private ? 'Private' : 'Public'}
                        </Badge>
                      </label>
                    </li>
                  ))}
                </ul>
              )}
            </div>

            {!allRepos && (
              <p className="text-xs text-muted">
                <span className="font-medium text-secondary">{selected.length}</span>{' '}
                selected
              </p>
            )}
          </div>
        </Card>
      ) : (
        <Card>
          <CardHeader
            title="Selective scope"
            description="Back up only part of a local checkout"
          />
          <div className="space-y-4 px-5 py-4">
            <Field
              label="Local checkout folder"
              htmlFor="checkout-root"
              hint="The working copy (or subfolder) you want to capture."
            >
              <div className="flex gap-2">
                <Input
                  id="checkout-root"
                  value={checkoutRoot}
                  onChange={(e) => setCheckoutRoot(e.target.value)}
                  placeholder="/path/to/checkout"
                />
                <Button
                  onClick={() => browse(setCheckoutRoot)}
                  aria-label="Browse for checkout folder"
                >
                  <FolderOpen size={14} aria-hidden />
                  Browse
                </Button>
              </div>
            </Field>

            <div className="grid grid-cols-1 gap-4 md:grid-cols-2">
              <Field
                label="Include patterns"
                htmlFor="include-patterns"
                hint="One per line, e.g. src/**, docs/**, *.md"
              >
                <textarea
                  id="include-patterns"
                  value={includePatterns}
                  onChange={(e) => setIncludePatterns(e.target.value)}
                  rows={4}
                  className="w-full rounded-lg border border-border bg-white px-3 py-2 font-mono text-xs text-foreground placeholder:text-muted transition-colors duration-150 focus:border-transparent focus:outline-none focus:ring-2 focus:ring-primary"
                  placeholder={'src/**\n*.md'}
                />
              </Field>
              <Field
                label="Exclude patterns"
                htmlFor="exclude-patterns"
                hint="One per line, e.g. node_modules/**, *.log"
              >
                <textarea
                  id="exclude-patterns"
                  value={excludePatterns}
                  onChange={(e) => setExcludePatterns(e.target.value)}
                  rows={4}
                  className="w-full rounded-lg border border-border bg-white px-3 py-2 font-mono text-xs text-foreground placeholder:text-muted transition-colors duration-150 focus:border-transparent focus:outline-none focus:ring-2 focus:ring-primary"
                  placeholder={'node_modules/**\n*.log'}
                />
              </Field>
            </div>

            <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
              <div className="flex items-center justify-between rounded-lg border border-border bg-surface px-4 py-3">
                <div>
                  <p className="text-sm font-medium text-foreground">Include hidden files</p>
                  <p className="text-xs text-muted">Dotfiles such as .github/ or .config/.</p>
                </div>
                <Toggle
                  checked={includeHidden}
                  onChange={setIncludeHidden}
                  label="Include hidden files"
                />
              </div>
              <div className="flex items-center justify-between rounded-lg border border-border bg-surface px-4 py-3">
                <div>
                  <p className="text-sm font-medium text-foreground">Include ignored files</p>
                  <p className="text-xs text-muted">Files matched by .gitignore.</p>
                </div>
                <Toggle
                  checked={includeIgnored}
                  onChange={setIncludeIgnored}
                  label="Include ignored files"
                />
              </div>
            </div>

            {hits.length > 0 && (
              <Notice tone="warning" title="Sensitive files detected">
                <p>
                  The following configured paths look like they contain secrets. Backing them up
                  stores credentials in plain form inside the backup folder.
                </p>
                <ul className="mt-2 list-inside list-disc space-y-0.5">
                  {hits.map((h) => (
                    <li key={`${h.kind}-${h.path}`}>
                      <span className="font-medium">{h.kind}</span>: {h.path}
                    </li>
                  ))}
                </ul>
                <label className="mt-3 flex cursor-pointer items-start gap-2.5">
                  <Checkbox
                    checked={sensitiveAck}
                    onChange={(e) => setSensitiveAck(e.target.checked)}
                    className="mt-0.5"
                  />
                  <span className="font-medium">
                    I understand these files contain secrets
                  </span>
                </label>
              </Notice>
            )}
          </div>
        </Card>
      )}

      <div className="flex flex-wrap items-center justify-between gap-3">
        <p className="flex items-center gap-1.5 text-xs text-muted">
          <AlertTriangle size={13} aria-hidden />
          Backups are written to local storage only — nothing is uploaded unless you publish
          separately.
        </p>
        <div className="flex items-center gap-2">
          <Button variant="ghost" onClick={() => navigate('repositories')}>
            Change selection
          </Button>
          <Button
            variant="primary"
            size="lg"
            onClick={start}
            disabled={starting || !!error}
          >
            <Play size={15} aria-hidden />
            {starting ? 'Starting…' : 'Start backup'}
          </Button>
        </div>
      </div>

      <p className="flex items-center gap-1.5 text-xs text-muted">
        <Save size={13} aria-hidden />
        A backup job appears in the Task Center as soon as it starts.
      </p>
    </div>
  );
}
