import { Check, Copy, Download, ScrollText, Search } from 'lucide-react';
import { useMemo, useState } from 'react';
import { logs_export, logs_list } from '../api/client';
import type { LogEntry } from '../api/types';
import {
  Badge,
  Button,
  Card,
  EmptyState,
  ErrorBanner,
  Input,
  Notice,
  PageLoading,
} from '../components/ui';
import { errMsg, formatDate } from '../lib/format';
import { useAsync, useDebouncedValue } from '../lib/hooks';
import { useNav } from '../store/nav';

const RENDER_CAP = 500;
const FETCH_LIMIT = 1000;

const LEVELS = ['all', 'debug', 'info', 'warn', 'error'] as const;
type Level = (typeof LEVELS)[number];

function levelTone(level: string): 'error' | 'warning' | 'neutral' {
  switch (level) {
    case 'error':
      return 'error';
    case 'warn':
    case 'warning':
      return 'warning';
    default:
      return 'neutral';
  }
}

function toText(entries: LogEntry[]): string {
  return entries
    .map((e) => {
      const base = `[${e.ts}] ${e.level} ${e.source}: ${e.message}`;
      return e.details ? `${base} | ${e.details}` : base;
    })
    .join('\n');
}

function download(filename: string, content: string): void {
  const blob = new Blob([content], { type: 'text/plain;charset=utf-8' });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  a.click();
  URL.revokeObjectURL(url);
}

export default function ActivityLogs() {
  const { navigate } = useNav();
  const [level, setLevel] = useState<Level>('all');
  const [searchInput, setSearchInput] = useState('');
  const search = useDebouncedValue(searchInput, 350);

  const { data: entries, error, loading, reload } = useAsync(
    () => logs_list({ level: level === 'all' ? '' : level, search, limit: FETCH_LIMIT }),
    [level, search],
  );

  const [copied, setCopied] = useState(false);
  const [exporting, setExporting] = useState(false);
  const [info, setInfo] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);

  const rows = useMemo(() => (entries ?? []).slice(0, RENDER_CAP), [entries]);
  const hidden = (entries ?? []).length - rows.length;

  async function copyVisible() {
    setActionError(null);
    try {
      await navigator.clipboard.writeText(toText(rows));
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch (e) {
      setActionError(errMsg(e));
    }
  }

  async function exportLogs() {
    setActionError(null);
    setInfo(null);
    setExporting(true);
    try {
      // Backend writes its own export file, then we also hand the loaded
      // entries to the browser as a .txt download.
      const path = await logs_export();
      download(`gbr-activity-${new Date().toISOString().slice(0, 10)}.txt`, toText(rows));
      setInfo(`Log file written to ${path} — a copy of the ${rows.length} loaded entries was also downloaded.`);
    } catch (e) {
      setActionError(errMsg(e));
    } finally {
      setExporting(false);
    }
  }

  if (loading && !entries) return <PageLoading rows={8} />;

  return (
    <div className="space-y-4">
      <ErrorBanner error={error ?? actionError} onRetry={error ? reload : undefined} />
      {info && (
        <Notice tone="success" title="Export complete">
          {info}
        </Notice>
      )}

      {/* Toolbar */}
      <Card className="p-4">
        <div className="flex flex-wrap items-center gap-3">
          <div className="flex items-center gap-1.5" role="group" aria-label="Filter by level">
            {LEVELS.map((l) => (
              <button
                key={l}
                type="button"
                onClick={() => setLevel(l)}
                aria-pressed={level === l}
                className={`rounded-full border px-3 py-1 text-xs font-medium transition-all duration-150 ${
                  level === l
                    ? 'border-[#F3E3A0] bg-pale text-foreground'
                    : 'border-border bg-white text-secondary hover:bg-surface'
                }`}
              >
                {l === 'all' ? 'All levels' : l}
              </button>
            ))}
          </div>

          <div className="relative min-w-[220px] flex-1">
            <Search
              size={14}
              className="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-muted"
              aria-hidden
            />
            <Input
              value={searchInput}
              onChange={(e) => setSearchInput(e.target.value)}
              placeholder="Search message, source or details…"
              aria-label="Search logs"
              className="pl-8"
            />
          </div>

          <Button onClick={copyVisible} disabled={rows.length === 0}>
            {copied ? <Check size={14} aria-hidden /> : <Copy size={14} aria-hidden />}
            {copied ? 'Copied' : 'Copy'}
          </Button>
          <Button variant="primary" onClick={exportLogs} disabled={exporting || rows.length === 0}>
            <Download size={14} aria-hidden />
            {exporting ? 'Exporting…' : 'Export'}
          </Button>
        </div>
      </Card>

      {/* Entries */}
      {!error && (entries ?? []).length === 0 && (
        <EmptyState
          icon={ScrollText}
          title="No log entries"
          description={
            search
              ? 'Nothing matches your search. Clear the search box or pick a different level.'
              : 'Activity from backups, restores and syncs is recorded here. Change the logging level in Settings if entries are missing.'
          }
          action={
            search ? (
              <Button onClick={() => setSearchInput('')}>Clear search</Button>
            ) : (
              <Button onClick={() => navigate('settings')}>Open Settings</Button>
            )
          }
        />
      )}

      {!error && rows.length > 0 && (
        <Card>
          <div className="flex items-center justify-between border-b border-border px-5 py-3">
            <p className="text-xs font-semibold uppercase tracking-wide text-muted">
              {(entries ?? []).length} entries
              {hidden > 0 && ` · showing the newest ${RENDER_CAP}`}
            </p>
            <p className="text-xs text-muted">Newest first</p>
          </div>
          <ul className="max-h-[600px] divide-y divide-[#EAEAEA] overflow-y-auto">
            {rows.map((e) => (
              <li key={e.id} className="flex items-start gap-3 px-5 py-2.5">
                <Badge tone={levelTone(e.level)}>{e.level}</Badge>
                <div className="min-w-0 flex-1">
                  <p className="break-words text-sm text-foreground">{e.message}</p>
                  {e.details && (
                    <p className="mt-0.5 break-words font-mono text-[11px] text-muted">
                      {e.details}
                    </p>
                  )}
                </div>
                <div className="shrink-0 text-right">
                  <p className="text-xs text-muted">{e.source}</p>
                  <p className="text-[11px] text-muted">{formatDate(e.ts)}</p>
                </div>
              </li>
            ))}
          </ul>
        </Card>
      )}
    </div>
  );
}
