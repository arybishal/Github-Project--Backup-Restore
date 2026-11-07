import { ListChecks, Play, XCircle } from 'lucide-react';
import { useEffect, useState } from 'react';
import { onTaskProgress, task_cancel, tasks_list } from '../api/client';
import type { TaskDto, TaskProgress, TaskStatus } from '../api/types';
import {
  Badge,
  Button,
  Card,
  CardHeader,
  EmptyState,
  ErrorBanner,
  PageLoading,
  Progress,
} from '../components/ui';
import { errMsg, formatDuration, formatRelative } from '../lib/format';
import { useAsync } from '../lib/hooks';
import { useNav } from '../store/nav';

function statusTone(status: TaskStatus): 'success' | 'error' | 'warning' | 'neutral' {
  switch (status) {
    case 'succeeded':
      return 'success';
    case 'failed':
      return 'error';
    case 'running':
      return 'warning';
    default:
      return 'neutral';
  }
}

function elapsedOf(task: TaskDto, live?: TaskProgress): number {
  if (live) return live.elapsed_ms;
  const start = new Date(task.created_at).getTime();
  const end = task.finished_at ? new Date(task.finished_at).getTime() : Date.now();
  return Math.max(0, end - start);
}

export default function TaskCenter() {
  const { navigate } = useNav();
  const { data: tasks, error, loading, reload } = useAsync(() => tasks_list(), []);
  const [liveMap, setLiveMap] = useState<Record<string, TaskProgress>>({});
  const [actionError, setActionError] = useState<string | null>(null);
  const [cancelling, setCancelling] = useState<string | null>(null);

  // Poll so status transitions show up even without an event.
  useEffect(() => {
    const timer = setInterval(() => reload(), 1500);
    return () => clearInterval(timer);
  }, [reload]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let alive = true;
    void onTaskProgress((p) => {
      if (alive) setLiveMap((m) => ({ ...m, [p.task_id]: p }));
    }).then((u) => {
      if (alive) unlisten = u;
      else u();
    });
    return () => {
      alive = false;
      unlisten?.();
    };
  }, []);

  async function cancel(id: string) {
    setActionError(null);
    setCancelling(id);
    try {
      await task_cancel({ id });
      reload();
    } catch (e) {
      setActionError(errMsg(e));
    } finally {
      setCancelling(null);
    }
  }

  if (loading && !tasks) return <PageLoading rows={6} />;

  const all = tasks ?? [];
  const running = all.filter((t) => t.status === 'running' || t.status === 'queued');
  const history = all.filter((t) => t.status !== 'running' && t.status !== 'queued');

  return (
    <div className="space-y-4">
      <ErrorBanner error={error ?? actionError} onRetry={error ? reload : undefined} />

      {!error && all.length === 0 && (
        <EmptyState
          icon={ListChecks}
          title="No tasks yet"
          description="Start a backup, restore or publish and the job will appear here with live progress, stage information and a cancel button."
          action={
            <div className="flex gap-2">
              <Button variant="primary" onClick={() => navigate('backup')}>
                <Play size={14} aria-hidden />
                Start a backup
              </Button>
              <Button onClick={() => navigate('publisher')}>Publish a project</Button>
            </div>
          }
        />
      )}

      {running.length > 0 && (
        <div className="space-y-3">
          <h2 className="text-sm font-semibold text-foreground">In progress</h2>
          {running.map((t) => {
            const live = liveMap[t.id];
            const percent = live?.percent ?? t.progress;
            return (
              <Card key={t.id} className="p-4">
                <div className="flex flex-wrap items-start justify-between gap-3">
                  <div className="min-w-0">
                    <p className="truncate text-sm font-semibold text-foreground">{t.title}</p>
                    <p className="mt-0.5 truncate text-xs text-muted">
                      {live?.stage ?? t.detail ?? t.kind}
                      {live?.current_repo ? ` · ${live.current_repo}` : ''}
                    </p>
                  </div>
                  <div className="flex items-center gap-3">
                    <span className="text-sm font-semibold text-foreground">
                      {Math.round(percent)}%
                    </span>
                    <Button
                      size="sm"
                      variant="outline"
                      onClick={() => cancel(t.id)}
                      disabled={cancelling === t.id}
                      aria-label={`Cancel ${t.title}`}
                    >
                      <XCircle size={13} aria-hidden />
                      {cancelling === t.id ? 'Cancelling…' : 'Cancel'}
                    </Button>
                  </div>
                </div>
                <div className="mt-3">
                  <Progress value={percent} label={t.title} />
                </div>
                <div className="mt-2 flex flex-wrap items-center justify-between gap-2 text-xs text-muted">
                  <span>
                    <Badge tone={statusTone(t.status)}>{t.status}</Badge>
                  </span>
                  <span>Elapsed {formatDuration(elapsedOf(t, live))}</span>
                </div>
              </Card>
            );
          })}
        </div>
      )}

      {history.length > 0 && (
        <Card>
          <CardHeader
            title="Task history"
            description="Finished jobs from this session and earlier"
            actions={
              <Button size="sm" variant="ghost" onClick={reload}>
                Refresh
              </Button>
            }
          />
          <ul className="divide-y divide-[#EAEAEA]">
            {history.map((t) => (
              <li key={t.id} className="flex flex-wrap items-center gap-3 px-5 py-3">
                <div className="min-w-0 flex-1">
                  <p className="truncate text-sm font-medium text-foreground">{t.title}</p>
                  <p className="truncate text-xs text-muted">
                    {t.kind} · started {formatRelative(t.created_at)}
                    {t.finished_at ? ` · finished ${formatRelative(t.finished_at)}` : ''}
                    {t.error ? ` · ${t.error}` : ''}
                  </p>
                </div>
                <span className="text-xs text-muted">
                  {Math.round(t.progress)}%
                </span>
                <Badge tone={statusTone(t.status)}>{t.status}</Badge>
              </li>
            ))}
          </ul>
        </Card>
      )}
    </div>
  );
}
