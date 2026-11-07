import {
  Activity,
  CheckCircle2,
  Clock,
  FolderGit2,
  FolderOpen,
  History,
  RotateCcw,
  Save,
  ShieldCheck,
  UploadCloud,
  User,
  XCircle,
  type LucideIcon,
} from 'lucide-react';
import { useEffect, useState } from 'react';
import { dashboard_stats, onTaskProgress } from '../api/client';
import type { TaskProgress } from '../api/types';
import {
  Badge,
  Button,
  Card,
  CardHeader,
  EmptyState,
  ErrorBanner,
  PageLoading,
  Progress,
  StatCard,
} from '../components/ui';
import { formatBytes, formatDate, formatDuration, formatRelative } from '../lib/format';
import { useAsync } from '../lib/hooks';
import { useNav, type PageId } from '../store/nav';

const QUICK_ACTIONS: { label: string; icon: LucideIcon; page: PageId; primary?: boolean }[] = [
  { label: 'Backup All', icon: Save, page: 'backup', primary: true },
  { label: 'Select Repositories', icon: FolderGit2, page: 'repositories' },
  { label: 'Restore Backup', icon: RotateCcw, page: 'restore' },
  { label: 'Add Local Project', icon: FolderOpen, page: 'local-projects' },
  { label: 'Publish to GitHub', icon: UploadCloud, page: 'publisher' },
];

function statusTone(status: string): 'success' | 'error' | 'warning' | 'neutral' {
  const s = status.toLowerCase();
  if (['succeeded', 'success', 'ok', 'verified'].includes(s)) return 'success';
  if (['failed', 'error'].includes(s)) return 'error';
  if (['running', 'pending', 'queued'].includes(s)) return 'warning';
  return 'neutral';
}

/** Log levels: only warn/error get colour; info/debug stay quiet. */
function logTone(level: string): 'error' | 'warning' | 'neutral' {
  const l = level.toLowerCase();
  if (l === 'error') return 'error';
  if (l === 'warn' || l === 'warning') return 'warning';
  return 'neutral';
}

export default function Dashboard() {
  const { navigate } = useNav();
  const { data, error, loading, reload } = useAsync(() => dashboard_stats(), []);
  const [live, setLive] = useState<TaskProgress | null>(null);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let alive = true;
    void onTaskProgress((p) => {
      if (alive) setLive(p);
    }).then((u) => {
      if (alive) unlisten = u;
      else u();
    });
    return () => {
      alive = false;
      unlisten?.();
    };
  }, []);

  if (loading && !data) return <PageLoading />;
  if (error) {
    return (
      <div className="space-y-4">
        <ErrorBanner error={error} onRetry={reload} />
      </div>
    );
  }
  if (!data) return null;

  const {
    account,
    repos,
    total_backups,
    verified_backups,
    failed_backups,
    last_success,
    storage_bytes,
    pending_tasks,
    recent_activity,
    recent_backups,
    health,
  } = data;

  const verifiedPct =
    total_backups > 0 ? Math.round((verified_backups / total_backups) * 100) : 0;

  return (
    <div className="space-y-4">
      {/* Live task progress (task_progress event) */}
      {live && (
        <Card className="p-4">
          <div className="flex flex-wrap items-center justify-between gap-2">
            <div className="min-w-0">
              <p className="truncate text-sm font-semibold text-foreground">{live.title}</p>
              <p className="mt-0.5 truncate text-xs text-muted">
                {live.stage}
                {live.current_repo ? ` · ${live.current_repo}` : ''}
                {` · ${formatDuration(live.elapsed_ms)}`}
              </p>
            </div>
            <span className="text-sm font-semibold text-foreground">
              {live.percent != null ? `${Math.round(live.percent)}%` : '…'}
            </span>
          </div>
          <div className="mt-3">
            <Progress value={live.percent} label={live.title} />
          </div>
        </Card>
      )}

      {/* Connected account */}
      <Card className="flex flex-wrap items-center gap-4 p-5">
        {account ? (
          <>
            {account.avatar_url ? (
              <img
                src={account.avatar_url}
                alt=""
                className="h-12 w-12 rounded-full border border-border bg-surface"
              />
            ) : (
              <div className="flex h-12 w-12 items-center justify-center rounded-full border border-border bg-surface">
                <User size={20} className="text-muted" aria-hidden />
              </div>
            )}
            <div className="min-w-0 flex-1">
              <p className="truncate text-sm font-semibold text-foreground">{account.username}</p>
              <p className="truncate text-xs text-muted">
                Connected via {account.auth_method}
                {account.scopes ? ` · scopes: ${account.scopes}` : ''}
              </p>
            </div>
            <Badge tone="success">Connected</Badge>
            <Button size="sm" onClick={() => navigate('settings')}>
              Manage account
            </Button>
          </>
        ) : (
          <>
            <div className="flex h-12 w-12 items-center justify-center rounded-full border border-border bg-surface">
              <User size={20} className="text-muted" aria-hidden />
            </div>
            <div className="min-w-0 flex-1">
              <p className="text-sm font-semibold text-foreground">Not connected</p>
              <p className="text-xs text-muted">
                Connect GitHub to sync repositories and publish backups.
              </p>
            </div>
            <Button variant="primary" onClick={() => navigate('settings')}>
              Connect GitHub
            </Button>
          </>
        )}
      </Card>

      {/* Repository stats */}
      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 xl:grid-cols-4">
        <StatCard label="Total repositories" value={repos.total} icon={FolderGit2} />
        <StatCard label="Public" value={repos.public} icon={FolderGit2} />
        <StatCard label="Private" value={repos.private} icon={FolderGit2} />
        <StatCard label="Organization" value={repos.organization} icon={FolderGit2} />
      </div>

      {/* Backup stats + health */}
      <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
        <Card>
          <CardHeader
            title="Backup statistics"
            description="Overall state of your backups"
            actions={
              <Button size="sm" variant="ghost" onClick={() => navigate('history')}>
                View history
              </Button>
            }
          />
          <dl className="divide-y divide-[#EAEAEA] px-5">
            <div className="flex items-center justify-between py-3">
              <dt className="text-sm text-secondary">Total backups</dt>
              <dd className="text-sm font-semibold text-foreground">{total_backups}</dd>
            </div>
            <div className="flex items-center justify-between py-3">
              <dt className="text-sm text-secondary">Last successful backup</dt>
              <dd className="text-sm font-medium text-foreground">
                {last_success ? formatRelative(last_success) : 'Never'}
                {last_success && (
                  <span className="ml-1.5 text-xs font-normal text-muted">
                    ({formatDate(last_success)})
                  </span>
                )}
              </dd>
            </div>
            <div className="flex items-center justify-between py-3">
              <dt className="text-sm text-secondary">Storage used</dt>
              <dd className="text-sm font-semibold text-foreground">
                {formatBytes(storage_bytes)}
              </dd>
            </div>
            <div className="flex items-center justify-between py-3">
              <dt className="text-sm text-secondary">Failed backups</dt>
              <dd
                className={
                  failed_backups > 0
                    ? 'text-sm font-semibold text-error'
                    : 'text-sm font-semibold text-foreground'
                }
              >
                {failed_backups}
              </dd>
            </div>
            <div className="flex items-center justify-between py-3">
              <dt className="text-sm text-secondary">Pending tasks</dt>
              <dd className="text-sm font-semibold text-foreground">{pending_tasks}</dd>
            </div>
          </dl>
        </Card>

        <Card>
          <CardHeader
            title="Backup health"
            description="Verification status across all backups"
            actions={
              <Button size="sm" variant="ghost" onClick={() => navigate('storage')}>
                Manage storage
              </Button>
            }
          />
          <div className="space-y-4 px-5 py-4">
            <div className="flex items-center justify-between text-sm">
              <span className="text-secondary">Verified backups</span>
              <span className="font-semibold text-foreground">
                {verified_backups} / {total_backups} ({verifiedPct}%)
              </span>
            </div>
            <Progress value={verifiedPct} label="Verified backups" />
            <div className="grid grid-cols-3 gap-3">
              <div className="rounded-lg border border-border bg-surface p-3 text-center">
                <p className="text-lg font-semibold text-foreground">{health.ok}</p>
                <p className="mt-0.5 flex items-center justify-center gap-1 text-xs text-muted">
                  <CheckCircle2 size={12} className="text-success" aria-hidden /> Healthy
                </p>
              </div>
              <div className="rounded-lg border border-border bg-surface p-3 text-center">
                <p className="text-lg font-semibold text-foreground">{health.unverified}</p>
                <p className="mt-0.5 flex items-center justify-center gap-1 text-xs text-muted">
                  <Clock size={12} className="text-warning" aria-hidden /> Unverified
                </p>
              </div>
              <div className="rounded-lg border border-border bg-surface p-3 text-center">
                <p className="text-lg font-semibold text-foreground">{health.failed}</p>
                <p className="mt-0.5 flex items-center justify-center gap-1 text-xs text-muted">
                  <XCircle size={12} className="text-error" aria-hidden /> Failed
                </p>
              </div>
            </div>
            <p className="flex items-start gap-2 text-xs leading-relaxed text-muted">
              <ShieldCheck size={14} className="mt-0.5 shrink-0" aria-hidden />
              Verification compares file counts and SHA-256 checksums recorded in each backup
              manifest against the stored payload.
            </p>
          </div>
        </Card>
      </div>

      {/* Quick actions */}
      <Card>
        <CardHeader title="Quick actions" description="Jump straight to a task" />
        <div className="flex flex-wrap gap-3 px-5 py-4">
          {QUICK_ACTIONS.map((a) => (
            <Button
              key={a.label}
              variant={a.primary ? 'primary' : 'outline'}
              onClick={() => navigate(a.page)}
            >
              <a.icon size={15} aria-hidden />
              {a.label}
            </Button>
          ))}
        </div>
      </Card>

      {/* Recent backups + recent activity */}
      <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
        <Card>
          <CardHeader
            title="Recent backups"
            description="Latest backup jobs"
            actions={
              <Button size="sm" variant="ghost" onClick={() => navigate('history')}>
                View all
              </Button>
            }
          />
          {recent_backups.length === 0 ? (
            <div className="px-5 py-6">
              <EmptyState
                icon={Save}
                title="No backups yet"
                description="Run your first backup from the Backup page — it will show up here with its verification status."
                action={
                  <Button variant="primary" size="sm" onClick={() => navigate('backup')}>
                    Start a backup
                  </Button>
                }
              />
            </div>
          ) : (
            <ul className="divide-y divide-[#EAEAEA]">
              {recent_backups.slice(0, 6).map((b) => (
                <li key={b.id} className="flex items-center gap-3 px-5 py-3">
                  <div className="min-w-0 flex-1">
                    <p className="truncate text-sm font-medium text-foreground">
                      {b.repo_full_name}
                    </p>
                    <p className="truncate text-xs text-muted">
                      {b.mode === 'full' ? 'Complete' : 'Selective'}
                      {b.format ? ` · ${b.format}` : ''} · {formatBytes(b.size_bytes)} ·{' '}
                      {formatRelative(b.created_at)}
                    </p>
                  </div>
                  <Badge tone={statusTone(b.status)}>{b.status}</Badge>
                  {b.verified && <Badge tone="yellow">Verified</Badge>}
                </li>
              ))}
            </ul>
          )}
        </Card>

        <Card>
          <CardHeader
            title="Recent activity"
            description="Latest events recorded by the app"
            actions={
              <Button size="sm" variant="ghost" onClick={() => navigate('logs')}>
                Open logs
              </Button>
            }
          />
          {recent_activity.length === 0 ? (
            <div className="px-5 py-6">
              <EmptyState
                icon={Activity}
                title="No activity yet"
                description="Events from backups, restores and syncs will appear here as soon as you start using the app."
              />
            </div>
          ) : (
            <ul className="divide-y divide-[#EAEAEA]">
              {recent_activity.slice(0, 6).map((log) => (
                <li key={log.id} className="flex items-start gap-3 px-5 py-3">
                  <Badge tone={logTone(log.level)}>{log.level}</Badge>
                  <div className="min-w-0 flex-1">
                    <p className="break-words text-sm text-foreground">{log.message}</p>
                    <p className="mt-0.5 text-xs text-muted">
                      {log.source} · {formatRelative(log.ts)}
                    </p>
                  </div>
                </li>
              ))}
            </ul>
          )}
        </Card>
      </div>

      {/* History shortcut row */}
      <div className="flex justify-end">
        <Button variant="ghost" size="sm" onClick={() => navigate('history')}>
          <History size={14} aria-hidden />
          Open backup history
        </Button>
      </div>
    </div>
  );
}
