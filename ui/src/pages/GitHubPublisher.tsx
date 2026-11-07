import {
  AlertOctagon,
  CheckCircle2,
  Copy,
  Eye,
  FileCode2,
  FolderOpen,
  GitBranch,
  UploadCloud,
  XCircle,
} from 'lucide-react';
import { useEffect, useState } from 'react';
import { onTaskProgress, pick_directory, publish_preview, publish_start } from '../api/client';
import type { TaskProgress } from '../api/types';
import {
  Badge,
  Button,
  Card,
  CardHeader,
  Checkbox,
  Dialog,
  EmptyState,
  ErrorBanner,
  Field,
  Input,
  Notice,
  PageLoading,
  Progress,
  Toggle,
} from '../components/ui';
import { errMsg, formatBytes, formatDuration } from '../lib/format';
import { useAsync } from '../lib/hooks';
import { getPublishPath, setPublishPath } from '../store/selection';

function basename(p: string): string {
  const parts = p.split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] ?? 'new-repository';
}

export default function GitHubPublisher() {
  const [path, setPath] = useState<string>(() => getPublishPath() ?? '');
  const { data: preview, error: previewError, loading: previewLoading, reload } = useAsync(
    () => (path ? publish_preview({ path }) : Promise.resolve(null)),
    [path],
  );

  const [repoName, setRepoName] = useState('');
  const [description, setDescription] = useState('');
  const [isPrivate, setIsPrivate] = useState(true);
  const [autoInit, setAutoInit] = useState(false);
  const [commitMessage, setCommitMessage] = useState('Initial commit');
  const [allowForce, setAllowForce] = useState(false);
  const [forceConfirmOpen, setForceConfirmOpen] = useState(false);

  const [publishing, setPublishing] = useState(false);
  const [publishError, setPublishError] = useState<string | null>(null);
  const [taskId, setTaskId] = useState<string | null>(null);
  const [live, setLive] = useState<TaskProgress | null>(null);
  const [namePrefilled, setNamePrefilled] = useState(false);
  const [copied, setCopied] = useState(false);

  // Live progress for the publish task.
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let alive = true;
    void onTaskProgress((p) => {
      if (alive && (!taskId || p.task_id === taskId)) setLive(p);
    }).then((u) => {
      if (alive) unlisten = u;
      else u();
    });
    return () => {
      alive = false;
      unlisten?.();
    };
  }, [taskId]);

  // Prefill the repository name from the chosen folder, once per preview.
  useEffect(() => {
    if (preview && !namePrefilled) {
      setRepoName(basename(preview.path));
      setNamePrefilled(true);
    }
  }, [preview, namePrefilled]);

  async function browse() {
    setPublishError(null);
    try {
      const picked = await pick_directory();
      if (picked) {
        setPublishPath(picked);
        setPath(picked);
        setNamePrefilled(false);
        setTaskId(null);
      }
    } catch (e) {
      setPublishError(errMsg(e));
    }
  }

  async function doPublish() {
    if (!preview) return;
    setPublishError(null);
    setPublishing(true);
    try {
      const id = await publish_start({
        req: {
          path: preview.path,
          repo_name: repoName.trim(),
          description: description.trim() ? description.trim() : null,
          private: isPrivate,
          auto_init_readme: autoInit,
          commit_message: commitMessage.trim() || 'Initial commit',
          allow_force: allowForce,
        },
      });
      setTaskId(id);
      setForceConfirmOpen(false);
    } catch (e) {
      setPublishError(errMsg(e));
    } finally {
      setPublishing(false);
    }
  }

  function onPublishClick() {
    setPublishError(null);
    if (!preview) return;
    if (!repoName.trim()) {
      setPublishError('Repository name is required.');
      return;
    }
    if (allowForce) {
      setForceConfirmOpen(true);
      return;
    }
    void doPublish();
  }

  async function copyGitignore() {
    if (!preview) return;
    try {
      await navigator.clipboard.writeText(preview.suggested_gitignore);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      setCopied(false);
    }
  }

  const busy = previewLoading && !!path;

  return (
    <div className="space-y-4">
      <ErrorBanner error={previewError ?? publishError} onRetry={previewError ? reload : undefined} />

      {/* Path picker */}
      <Card className="p-4">
        <div className="flex flex-wrap items-end gap-3">
          <Field label="Project folder" htmlFor="publish-path" className="min-w-[260px] flex-1">
            <Input
              id="publish-path"
              value={path}
              onChange={(e) => {
                setPath(e.target.value);
                setNamePrefilled(false);
              }}
              placeholder="/path/to/project"
            />
          </Field>
          <Button onClick={browse} aria-label="Browse for project folder">
            <FolderOpen size={14} aria-hidden />
            Browse
          </Button>
          <Button
            variant="primary"
            onClick={() => reload()}
            disabled={!path.trim() || busy}
          >
            <Eye size={14} aria-hidden />
            {busy ? 'Analyzing…' : 'Analyze folder'}
          </Button>
        </div>
      </Card>

      {!path.trim() && (
        <EmptyState
          icon={UploadCloud}
          title="Choose a folder to publish"
          description="Pick a local project folder. The app checks whether it is a Git repository, lists large files and flags anything that looks like a secret before you create the GitHub repository."
        />
      )}

      {path.trim() && busy && <PageLoading rows={4} />}

      {path.trim() && !busy && !preview && !previewError && (
        <EmptyState
          icon={UploadCloud}
          title="Nothing analyzed yet"
          description="Press “Analyze folder” to run the pre-publish checks for this path."
          action={
            <Button variant="primary" onClick={() => reload()}>
              Analyze folder
            </Button>
          }
        />
      )}

      {preview && !busy && (
        <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
          {/* Pre-publish checklist */}
          <Card>
            <CardHeader
              title="Pre-publish checklist"
              description={preview.path}
              actions={
                preview.is_git_repo ? (
                  <Badge tone="success">Git detected</Badge>
                ) : (
                  <Badge tone="warning">Not a Git repository</Badge>
                )
              }
            />
            <div className="space-y-4 px-5 py-4">
              <ul className="space-y-2 text-sm">
                <li className="flex items-center gap-2">
                  {preview.is_git_repo ? (
                    <CheckCircle2 size={15} className="text-success" aria-hidden />
                  ) : (
                    <XCircle size={15} className="text-error" aria-hidden />
                  )}
                  <span className="text-secondary">Git repository detected</span>
                </li>
                <li className="flex items-center gap-2">
                  {preview.has_commits ? (
                    <CheckCircle2 size={15} className="text-success" aria-hidden />
                  ) : (
                    <XCircle size={15} className="text-error" aria-hidden />
                  )}
                  <span className="text-secondary">
                    {preview.has_commits
                      ? 'Has commits to push'
                      : 'No commits yet — an initial commit will be created'}
                  </span>
                </li>
                <li className="flex items-center gap-2">
                  <GitBranch size={15} className="text-muted" aria-hidden />
                  <span className="text-secondary">
                    Current branch:{' '}
                    <span className="font-medium text-foreground">
                      {preview.current_branch ?? '—'}
                    </span>
                  </span>
                </li>
                <li className="flex items-center gap-2">
                  <FileCode2 size={15} className="text-muted" aria-hidden />
                  <span className="text-secondary">
                    {preview.file_count} files · {formatBytes(preview.total_bytes)}
                  </span>
                </li>
              </ul>

              {preview.large_files.length > 0 && (
                <div className="rounded-lg border border-border bg-surface px-4 py-3">
                  <p className="text-xs font-semibold uppercase tracking-wide text-muted">
                    Large files ({preview.large_files.length})
                  </p>
                  <ul className="mt-2 max-h-32 space-y-1 overflow-y-auto text-xs text-secondary">
                    {preview.large_files.map((f) => (
                      <li key={f.path} className="flex justify-between gap-3">
                        <span className="truncate font-mono" title={f.path}>
                          {f.path}
                        </span>
                        <span className="shrink-0">{formatBytes(f.size)}</span>
                      </li>
                    ))}
                  </ul>
                  <p className="mt-2 text-xs text-muted">
                    GitHub rejects files over 100 MB; consider Git LFS.
                  </p>
                </div>
              )}

              {preview.secret_hits.length > 0 && (
                <div className="rounded-lg border border-[#FECACA] bg-[#FEF2F2] px-4 py-3">
                  <p className="flex items-center gap-1.5 text-xs font-semibold uppercase tracking-wide text-error">
                    <AlertOctagon size={12} aria-hidden />
                    Secrets found ({preview.secret_hits.length})
                  </p>
                  <ul className="mt-2 space-y-1 text-xs text-[#B91C1C]">
                    {preview.secret_hits.map((s) => (
                      <li key={`${s.path}-${s.kind}`} className="flex justify-between gap-3">
                        <span className="truncate font-mono" title={s.path}>
                          {s.path}
                        </span>
                        <span className="shrink-0 font-medium">{s.kind}</span>
                      </li>
                    ))}
                  </ul>
                  <p className="mt-2 text-xs font-medium text-[#B91C1C]">
                    Publishing these files exposes your credentials publicly. Remove or ignore
                    them first.
                  </p>
                </div>
              )}

              <div>
                <div className="mb-1.5 flex items-center justify-between">
                  <p className="text-xs font-semibold uppercase tracking-wide text-muted">
                    Suggested .gitignore
                  </p>
                  <Button size="sm" variant="ghost" onClick={copyGitignore}>
                    <Copy size={12} aria-hidden />
                    {copied ? 'Copied' : 'Copy'}
                  </Button>
                </div>
                <pre className="max-h-44 overflow-auto rounded-lg border border-border bg-[#FAFAFA] p-3 text-[11px] leading-relaxed text-foreground">
                  {preview.suggested_gitignore || '# nothing suggested'}
                </pre>
              </div>
            </div>
          </Card>

          {/* Publish form */}
          <Card>
            <CardHeader
              title="Create GitHub repository"
              description="All fields are sent to the backend as-is"
            />
            <div className="space-y-4 px-5 py-4">
              <Field label="Repository name" htmlFor="repo-name">
                <Input
                  id="repo-name"
                  value={repoName}
                  onChange={(e) => setRepoName(e.target.value)}
                  placeholder="my-project"
                />
              </Field>

              <Field label="Description" htmlFor="repo-description" hint="Optional.">
                <Input
                  id="repo-description"
                  value={description}
                  onChange={(e) => setDescription(e.target.value)}
                  placeholder="What is this project?"
                />
              </Field>

              <div className="flex items-center justify-between rounded-lg border border-border bg-surface px-4 py-3">
                <div>
                  <p className="text-sm font-medium text-foreground">
                    {isPrivate ? 'Private repository' : 'Public repository'}
                  </p>
                  <p className="text-xs text-muted">
                    {isPrivate
                      ? 'Only you (and collaborators you add) can see it.'
                      : 'Anyone on GitHub can see this repository.'}
                  </p>
                </div>
                <Toggle checked={isPrivate} onChange={setIsPrivate} label="Private repository" />
              </div>

              <label className="flex cursor-pointer items-start gap-2.5">
                <Checkbox
                  checked={autoInit}
                  onChange={(e) => setAutoInit(e.target.checked)}
                  className="mt-0.5"
                />
                <span>
                  <span className="block text-sm font-medium text-foreground">
                    Initialize with a README
                  </span>
                  <span className="block text-xs text-muted">
                    Useful when the folder has no commits yet.
                  </span>
                </span>
              </label>

              <Field label="Commit message" htmlFor="commit-message">
                <Input
                  id="commit-message"
                  value={commitMessage}
                  onChange={(e) => setCommitMessage(e.target.value)}
                />
              </Field>

              <div
                className={`rounded-lg border px-4 py-3 ${allowForce ? 'border-[#FDE68A] bg-[#FFFBEB]' : 'border-border bg-surface'}`}
              >
                <label className="flex cursor-pointer items-start gap-2.5">
                  <Checkbox
                    checked={allowForce}
                    onChange={(e) => setAllowForce(e.target.checked)}
                    className="mt-0.5"
                  />
                  <span>
                    <span className="block text-sm font-medium text-foreground">
                      Allow force push
                    </span>
                    <span className="block text-xs text-muted">
                      Off by default. Required only when the remote already has diverged history
                      you intend to overwrite.
                    </span>
                  </span>
                </label>
              </div>

              {preview.secret_hits.length > 0 && (
                <Notice tone="warning" title="Secrets will be pushed">
                  This folder still contains files that look like credentials. Publishing now
                  would expose them.
                </Notice>
              )}

              {/* Live progress */}
              {taskId && (
                <div className="rounded-lg border border-border bg-white p-4 shadow-soft">
                  <div className="flex items-center justify-between gap-2">
                    <p className="text-sm font-medium text-foreground">
                      {live?.title ?? 'Publishing…'}
                    </p>
                    <span className="text-sm font-semibold text-foreground">
                      {live?.percent != null ? `${Math.round(live.percent)}%` : ''}
                    </span>
                  </div>
                  <div className="mt-2">
                    <Progress value={live?.percent ?? null} label="Publish progress" />
                  </div>
                  <p className="mt-2 text-xs text-muted">
                    {live?.stage ?? 'Waiting for the backend…'}
                    {live?.current_repo ? ` · ${live.current_repo}` : ''}
                    {live ? ` · ${formatDuration(live.elapsed_ms)}` : ''}
                  </p>
                </div>
              )}

              <div className="flex justify-end border-t border-border pt-4">
                <Button
                  variant="primary"
                  size="lg"
                  onClick={onPublishClick}
                  disabled={publishing || !!previewError}
                >
                  <UploadCloud size={15} aria-hidden />
                  {publishing ? 'Starting…' : 'Publish to GitHub'}
                </Button>
              </div>
            </div>
          </Card>
        </div>
      )}

      {/* Extra confirmation for force push */}
      <Dialog
        open={forceConfirmOpen}
        onClose={() => setForceConfirmOpen(false)}
        title="Confirm force push"
        footer={
          <>
            <Button onClick={() => setForceConfirmOpen(false)} disabled={publishing}>
              Cancel
            </Button>
            <Button variant="danger" onClick={() => void doPublish()} disabled={publishing}>
              {publishing ? 'Starting…' : 'Yes, allow force push'}
            </Button>
          </>
        }
      >
        <p>
          You enabled <span className="font-medium text-foreground">force push</span>. The remote
          repository's history can be overwritten and commits that only exist on GitHub will be
          lost. Continue?
        </p>
      </Dialog>
    </div>
  );
}
