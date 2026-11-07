import { ExternalLink, Info, ShieldCheck } from 'lucide-react';
import { app_version, backup_history, backup_manifest } from '../api/client';
import {
  Badge,
  Button,
  Card,
  CardHeader,
  EmptyState,
  ErrorBanner,
  PageLoading,
} from '../components/ui';
import { useAsync } from '../lib/hooks';
import { useNav } from '../store/nav';

const COVERS = [
  'Full Git history, branches and tags of the repositories you select.',
  'Git LFS objects when git-lfs is available and LFS inclusion is enabled.',
  'A manifest per backup with SHA-256 checksums for every stored file.',
  'Verification: checksums can be re-run any time from Backup History.',
];

const DOES_NOT_COVER = [
  'GitHub-side data: issues, pull requests, Actions runs, Wikis, project boards and release assets.',
  'Files excluded by your backup settings or by the patterns of a selective backup.',
  'Anything outside the repositories and local folders you chose — always check the manifest.',
  'Off-machine copies: backups are written to local storage only unless you publish them yourself.',
];

export default function About() {
  const { navigate } = useNav();
  const { data: version, error: versionError, loading: versionLoading, reload } = useAsync(
    () => app_version(),
    [],
  );
  const {
    data: latestManifest,
    error: manifestError,
    loading: manifestLoading,
    reload: reloadManifest,
  } = useAsync(async () => {
    const list = await backup_history({ limit: 1 });
    if (list.length === 0) return null;
    return backup_manifest({ backupId: list[0].id });
  }, []);

  if (versionLoading) return <PageLoading rows={4} />;

  return (
    <div className="space-y-4">
      <ErrorBanner
        error={versionError ?? manifestError}
        onRetry={() => {
          reload();
          reloadManifest();
        }}
      />

      <Card className="p-6">
        <div className="flex flex-wrap items-start gap-5">
          <div className="flex h-14 w-14 shrink-0 items-center justify-center rounded-xl bg-primary">
            <Info size={26} className="text-foreground" aria-hidden />
          </div>
          <div className="min-w-0 flex-1">
            <h2 className="text-lg font-semibold text-foreground">Git Backup Restore</h2>
            <p className="mt-1 text-sm text-secondary">
              A desktop tool that mirrors your GitHub repositories and local projects to storage
              you control, with verifiable manifests and one-click restore.
            </p>
            <div className="mt-3 flex flex-wrap items-center gap-2">
              <Badge tone="yellow">Version {version || 'unknown'}</Badge>
              <Badge tone="neutral">Tauri 2 + React</Badge>
              <a
                href="https://bishalaryal.com.np"
                target="_blank"
                rel="noreferrer"
                className="inline-flex items-center gap-1 rounded-full border border-border bg-white px-2.5 py-0.5 text-[11px] font-medium text-foreground transition-colors duration-150 hover:bg-surface"
              >
                bishalaryal.com.np
                <ExternalLink size={11} aria-hidden />
              </a>
            </div>
          </div>
        </div>

        <dl className="mt-6 grid grid-cols-1 gap-x-8 gap-y-2 border-t border-border pt-4 text-sm sm:grid-cols-2">
          <div className="flex justify-between gap-4">
            <dt className="text-muted">Developer</dt>
            <dd className="font-medium text-foreground">Bishal Aryal</dd>
          </div>
          <div className="flex justify-between gap-4">
            <dt className="text-muted">Website</dt>
            <dd>
              <a
                href="https://bishalaryal.com.np"
                target="_blank"
                rel="noreferrer"
                className="font-medium text-foreground underline decoration-primary decoration-2 underline-offset-4"
              >
                https://bishalaryal.com.np
              </a>
            </dd>
          </div>
        </dl>
      </Card>

      <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
        <Card>
          <CardHeader
            title="What a backup covers"
            description="Honest summary — the manifest is the source of truth"
          />
          <ul className="space-y-2.5 px-5 py-4">
            {COVERS.map((c) => (
              <li key={c} className="flex items-start gap-2 text-sm text-secondary">
                <ShieldCheck size={15} className="mt-0.5 shrink-0 text-success" aria-hidden />
                {c}
              </li>
            ))}
          </ul>
        </Card>

        <Card>
          <CardHeader
            title="What it does not cover"
            description="Read this before relying on a backup"
          />
          <ul className="space-y-2.5 px-5 py-4">
            {DOES_NOT_COVER.map((c) => (
              <li key={c} className="flex items-start gap-2 text-sm text-secondary">
                <Info size={15} className="mt-0.5 shrink-0 text-warning" aria-hidden />
                {c}
              </li>
            ))}
          </ul>
        </Card>
      </div>

      <Card>
        <CardHeader
          title="Limitations recorded by your latest backup"
          description="Verbatim from BackupManifest.limitations"
        />
        <div className="px-5 py-4">
          {manifestLoading && <p className="text-sm text-muted">Loading latest manifest…</p>}
          {!manifestLoading && latestManifest && (
            <ul className="list-inside list-disc space-y-1.5 text-sm text-secondary">
              {latestManifest.limitations.map((l, i) => (
                <li key={i}>{l}</li>
              ))}
            </ul>
          )}
          {!manifestLoading && !latestManifest && !manifestError && (
            <EmptyState
              icon={Info}
              title="No backups yet"
              description="Every backup writes its own limitations into the manifest. Run one backup and the exact coverage list for it appears here."
              action={
                <Button variant="primary" onClick={() => navigate('backup')}>
                  Create a backup
                </Button>
              }
            />
          )}
        </div>
      </Card>

      <p className="text-center text-xs text-muted">
        Git Backup Restore stores data locally and does not transmit it anywhere unless you
        explicitly publish a repository to GitHub.
      </p>
    </div>
  );
}
