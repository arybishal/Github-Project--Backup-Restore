import { FolderOpen, FolderPlus, GitBranch, Trash2, UploadCloud } from 'lucide-react';
import { useState } from 'react';
import { local_projects_add, local_projects_list, local_projects_remove, pick_directory } from '../api/client';
import type { LocalProject } from '../api/types';
import {
  Badge,
  Button,
  Card,
  Dialog,
  EmptyState,
  ErrorBanner,
  PageLoading,
} from '../components/ui';
import { errMsg, formatDate } from '../lib/format';
import { useAsync } from '../lib/hooks';
import { useNav } from '../store/nav';
import { setPublishPath } from '../store/selection';

export default function LocalProjects() {
  const { navigate } = useNav();
  const { data: projects, error, loading, reload } = useAsync(() => local_projects_list(), []);

  const [adding, setAdding] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);
  const [removeTarget, setRemoveTarget] = useState<LocalProject | null>(null);
  const [removing, setRemoving] = useState(false);

  async function addProject() {
    setActionError(null);
    setAdding(true);
    try {
      const path = await pick_directory();
      if (path) {
        await local_projects_add({ path });
        reload();
      }
    } catch (e) {
      setActionError(errMsg(e));
    } finally {
      setAdding(false);
    }
  }

  async function confirmRemove() {
    if (!removeTarget) return;
    setRemoving(true);
    setActionError(null);
    try {
      await local_projects_remove({ path: removeTarget.path });
      setRemoveTarget(null);
      reload();
    } catch (e) {
      setActionError(errMsg(e));
    } finally {
      setRemoving(false);
    }
  }

  function publish(p: LocalProject) {
    setPublishPath(p.path);
    navigate('publisher');
  }

  if (loading && !projects) return <PageLoading rows={5} />;

  return (
    <div className="space-y-4">
      <ErrorBanner error={error ?? actionError} onRetry={error ? reload : undefined} />

      <div className="flex flex-wrap items-center justify-between gap-3">
        <p className="text-sm text-secondary">
          Local folders you want to publish or keep an eye on. They are stored as a list —
          nothing is copied.
        </p>
        <Button variant="primary" onClick={addProject} disabled={adding || !!error}>
          <FolderPlus size={15} aria-hidden />
          {adding ? 'Opening picker…' : 'Add local project'}
        </Button>
      </div>

      {!error && projects && projects.length === 0 && (
        <EmptyState
          icon={FolderOpen}
          title="No local projects yet"
          description="Add a folder from your machine to check whether it is a Git repository and to publish it to GitHub from the Publisher page."
          action={
            <Button variant="primary" onClick={addProject} disabled={adding}>
              <FolderPlus size={15} aria-hidden />
              Add local project
            </Button>
          }
        />
      )}

      {!error && projects && projects.length > 0 && (
        <Card>
          <ul className="divide-y divide-[#EAEAEA]">
            {projects.map((p) => (
              <li key={p.path} className="flex flex-wrap items-center gap-3 px-5 py-4">
                <div className="flex h-10 w-10 shrink-0 items-center justify-center rounded-lg border border-border bg-surface">
                  <FolderOpen size={16} className="text-muted" aria-hidden />
                </div>
                <div className="min-w-0 flex-1">
                  <p className="truncate text-sm font-medium text-foreground">{p.name}</p>
                  <p className="truncate font-mono text-xs text-muted" title={p.path}>
                    {p.path}
                  </p>
                </div>
                {p.is_git_repo ? (
                  <Badge tone="success">
                    <GitBranch size={10} aria-hidden />
                    Git repository
                  </Badge>
                ) : (
                  <Badge tone="warning">Not a Git repo</Badge>
                )}
                <span className="hidden text-xs text-muted sm:block">
                  Added {formatDate(p.added_at)}
                </span>
                <div className="flex items-center gap-1.5">
                  <Button size="sm" onClick={() => publish(p)} aria-label={`Publish ${p.name} to GitHub`}>
                    <UploadCloud size={13} aria-hidden />
                    Publish
                  </Button>
                  <Button
                    size="sm"
                    variant="ghost"
                    className="text-error hover:text-error"
                    onClick={() => setRemoveTarget(p)}
                    aria-label={`Remove ${p.name}`}
                  >
                    <Trash2 size={13} aria-hidden />
                  </Button>
                </div>
              </li>
            ))}
          </ul>
        </Card>
      )}

      <Dialog
        open={!!removeTarget}
        onClose={() => setRemoveTarget(null)}
        title="Remove local project"
        footer={
          <>
            <Button onClick={() => setRemoveTarget(null)} disabled={removing}>
              Cancel
            </Button>
            <Button variant="danger" onClick={confirmRemove} disabled={removing}>
              {removing ? 'Removing…' : 'Remove'}
            </Button>
          </>
        }
      >
        <p>
          Remove <span className="font-medium text-foreground">{removeTarget?.name}</span> from the
          list? The folder itself stays untouched on disk.
        </p>
      </Dialog>
    </div>
  );
}
