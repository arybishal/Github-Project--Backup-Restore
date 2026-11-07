import {
  CheckCircle2,
  KeyRound,
  LogOut,
  MonitorSmartphone,
  Save,
  ShieldCheck,
  XCircle,
} from 'lucide-react';
import { useEffect, useState } from 'react';
import {
  app_version,
  auth_device_complete,
  auth_device_start,
  auth_login_token,
  auth_logout,
  auth_status,
  git_test,
  pick_directory,
  settings_get,
  settings_set,
} from '../api/client';
import type { AccountInfo, DeviceCode, Settings as SettingsType } from '../api/types';
import {
  Badge,
  Button,
  Card,
  CardHeader,
  ErrorBanner,
  Field,
  Input,
  Notice,
  PageLoading,
  Segmented,
  Select,
  Toggle,
} from '../components/ui';
import { errMsg } from '../lib/format';
import { useAsync } from '../lib/hooks';

export default function Settings() {
  const { data, error, loading, reload } = useAsync(
    () => Promise.all([settings_get(), auth_status(), app_version()]),
    [],
  );

  const [draft, setDraft] = useState<SettingsType | null>(null);
  const [account, setAccount] = useState<AccountInfo | null>(null);
  const [version, setVersion] = useState('');
  const [hydrated, setHydrated] = useState(false);

  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);

  const [token, setToken] = useState('');
  const [tokenBusy, setTokenBusy] = useState(false);
  const [accountBusy, setAccountBusy] = useState(false);
  const [accountError, setAccountError] = useState<string | null>(null);

  const [device, setDevice] = useState<DeviceCode | null>(null);
  const [deviceBusy, setDeviceBusy] = useState(false);

  const [gitPath, setGitPath] = useState<{ ok: boolean; msg: string } | null>(null);
  const [testing, setTesting] = useState(false);

  useEffect(() => {
    if (data && !hydrated) {
      setDraft(data[0]);
      setAccount(data[1]);
      setVersion(data[2]);
      setHydrated(true);
    }
  }, [data, hydrated]);

  function patch(next: Partial<SettingsType>) {
    setDraft((d) => (d ? { ...d, ...next } : d));
    setSaved(false);
  }

  async function save() {
    if (!draft) return;
    setSaving(true);
    setSaved(false);
    setSaveError(null);
    try {
      await settings_set({ settings: draft });
      setSaved(true);
    } catch (e) {
      setSaveError(errMsg(e));
    } finally {
      setSaving(false);
    }
  }

  async function browseBackupRoot() {
    setSaveError(null);
    try {
      const path = await pick_directory();
      if (path) patch({ backup_root: path });
    } catch (e) {
      setSaveError(errMsg(e));
    }
  }

  async function connectToken() {
    setAccountError(null);
    setTokenBusy(true);
    try {
      const info = await auth_login_token({ token: token.trim() });
      setAccount(info);
      setToken('');
    } catch (e) {
      setAccountError(errMsg(e));
    } finally {
      setTokenBusy(false);
    }
  }

  async function startDevice() {
    setAccountError(null);
    setDeviceBusy(true);
    try {
      const code = await auth_device_start();
      setDevice(code);
    } catch (e) {
      setAccountError(errMsg(e));
    } finally {
      setDeviceBusy(false);
    }
  }

  async function completeDevice() {
    if (!device) return;
    setAccountError(null);
    setDeviceBusy(true);
    try {
      const info = await auth_device_complete({
        clientId: draft?.github_client_id ?? '',
        deviceCode: device.device_code,
      });
      setAccount(info);
      setDevice(null);
    } catch (e) {
      setAccountError(errMsg(e));
    } finally {
      setDeviceBusy(false);
    }
  }

  async function disconnect() {
    setAccountError(null);
    setAccountBusy(true);
    try {
      await auth_logout();
      setAccount(null);
    } catch (e) {
      setAccountError(errMsg(e));
    } finally {
      setAccountBusy(false);
    }
  }

  async function testGit() {
    setTesting(true);
    setGitPath(null);
    try {
      const v = await git_test();
      setGitPath({ ok: true, msg: `Detected ${v}` });
    } catch (e) {
      setGitPath({ ok: false, msg: errMsg(e) });
    } finally {
      setTesting(false);
    }
  }

  if (loading && !draft) return <PageLoading rows={6} />;

  return (
    <div className="space-y-4">
      <ErrorBanner error={error ?? saveError ?? accountError} onRetry={error ? reload : undefined} />

      {/* GitHub account */}
      <Card>
        <CardHeader
          title="GitHub account"
          description="Connect so repositories can be synced and published"
          actions={
            account && (
              <Badge tone="success">
                <ShieldCheck size={11} aria-hidden />
                Connected
              </Badge>
            )
          }
        />
        <div className="space-y-4 px-5 py-4">
          {account ? (
            <div className="flex flex-wrap items-center gap-4">
              {account.avatar_url ? (
                <img
                  src={account.avatar_url}
                  alt=""
                  className="h-11 w-11 rounded-full border border-border bg-surface"
                />
              ) : (
                <div className="flex h-11 w-11 items-center justify-center rounded-full border border-border bg-surface">
                  <KeyRound size={17} className="text-muted" aria-hidden />
                </div>
              )}
              <div className="min-w-0 flex-1">
                <p className="text-sm font-semibold text-foreground">{account.username}</p>
                <p className="truncate text-xs text-muted">
                  {account.auth_method}
                  {account.scopes ? ` · ${account.scopes}` : ''}
                </p>
              </div>
              <Button onClick={disconnect} disabled={accountBusy}>
                <LogOut size={14} aria-hidden />
                {accountBusy ? 'Disconnecting…' : 'Disconnect'}
              </Button>
            </div>
          ) : (
            <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
              {/* Token */}
              <div className="rounded-lg border border-border bg-surface p-4">
                <p className="text-sm font-medium text-foreground">Connect with a token</p>
                <p className="mt-0.5 text-xs text-muted">
                  Paste a GitHub personal access token with repo scope.
                </p>
                <div className="mt-3 flex gap-2">
                  <Field label="Personal access token" htmlFor="pat" className="flex-1">
                    <Input
                      id="pat"
                      type="password"
                      value={token}
                      onChange={(e) => setToken(e.target.value)}
                      placeholder="ghp_…"
                      autoComplete="off"
                    />
                  </Field>
                  <Button
                    variant="primary"
                    className="mt-6"
                    onClick={connectToken}
                    disabled={tokenBusy || !token.trim()}
                  >
                    {tokenBusy ? 'Connecting…' : 'Connect'}
                  </Button>
                </div>
              </div>

              {/* Device flow */}
              <div className="rounded-lg border border-border bg-surface p-4">
                <p className="text-sm font-medium text-foreground">Connect with device flow</p>
                <p className="mt-0.5 text-xs text-muted">
                  Opens github.com/login/device in your browser — no token pasted here.
                </p>
                {!device ? (
                  <Button className="mt-3" onClick={startDevice} disabled={deviceBusy}>
                    <MonitorSmartphone size={14} aria-hidden />
                    {deviceBusy ? 'Starting…' : 'Start device login'}
                  </Button>
                ) : (
                  <div className="mt-3 space-y-2">
                    <div className="rounded-lg border border-border bg-white px-4 py-3 text-center">
                      <p className="text-xs text-muted">Enter this code at</p>
                      <p className="mt-1 font-mono text-xl font-semibold tracking-widest text-foreground">
                        {device.user_code}
                      </p>
                      <a
                        href={device.verification_uri}
                        target="_blank"
                        rel="noreferrer"
                        className="mt-1 inline-block text-xs font-medium text-foreground underline decoration-primary decoration-2 underline-offset-2"
                      >
                        {device.verification_uri}
                      </a>
                    </div>
                    <div className="flex gap-2">
                      <Button
                        variant="primary"
                        onClick={completeDevice}
                        disabled={deviceBusy || !draft?.github_client_id}
                      >
                        {deviceBusy ? 'Checking…' : "I've authorized it"}
                      </Button>
                      <Button onClick={() => setDevice(null)} disabled={deviceBusy}>
                        Cancel
                      </Button>
                    </div>
                    {!draft?.github_client_id && (
                      <p className="text-xs text-warning">
                        Set your GitHub OAuth client id below first — the device flow needs it.
                      </p>
                    )}
                  </div>
                )}
              </div>
            </div>
          )}
        </div>
      </Card>

      {/* Backup defaults */}
      <Card>
        <CardHeader title="Backup defaults" description="Applied to new backup jobs" />
        <div className="grid grid-cols-1 gap-4 px-5 py-4 md:grid-cols-2">
          <Field
            label="Default backup directory"
            htmlFor="default-backup-dir"
            hint="Leave empty to use the app's default location."
            className="md:col-span-2"
          >
            <div className="flex gap-2">
              <Input
                id="default-backup-dir"
                value={draft?.backup_root ?? ''}
                onChange={(e) => patch({ backup_root: e.target.value })}
                placeholder="/path/to/backups"
              />
              <Button onClick={browseBackupRoot}>Browse</Button>
            </div>
          </Field>

          <div>
            <p className="mb-1.5 text-sm font-medium text-foreground">Default mode</p>
            <Segmented
              ariaLabel="Default backup mode"
              value={draft?.default_mode ?? 'full'}
              onChange={(v) => patch({ default_mode: v })}
              options={[
                { value: 'full', label: 'Complete' },
                { value: 'selective', label: 'Selective' },
              ]}
            />
          </div>

          <Field label="Default format" htmlFor="default-format">
            <Select
              id="default-format"
              value={draft?.default_format ?? 'folder'}
              onChange={(e) => patch({ default_format: e.target.value as SettingsType['default_format'] })}
            >
              <option value="folder">folder</option>
              <option value="zip">zip</option>
              <option value="tar">tar</option>
              <option value="tar.zst">tar.zst</option>
              <option value="mirror">mirror</option>
            </Select>
          </Field>

          <Field
            label={`Compression level — ${draft?.compression_level ?? 6}`}
            htmlFor="compression"
            hint="1–22. Higher values make smaller, slower archives."
          >
            <input
              id="compression"
              type="range"
              min={1}
              max={22}
              value={draft?.compression_level ?? 6}
              onChange={(e) => patch({ compression_level: Number(e.target.value) })}
              className="h-2 w-full cursor-pointer accent-primary"
            />
          </Field>

          <Field
            label="Snapshot retention"
            htmlFor="retention"
            hint="Number of snapshots to keep per repository."
          >
            <Input
              id="retention"
              type="number"
              min={0}
              value={draft?.snapshot_retention ?? 10}
              onChange={(e) => patch({ snapshot_retention: Number(e.target.value) })}
            />
          </Field>
        </div>
      </Card>

      {/* Git & GitHub app */}
      <Card>
        <CardHeader title="Git & GitHub app" description="Local git executable and OAuth app" />
        <div className="grid grid-cols-1 gap-4 px-5 py-4 md:grid-cols-2">
          <Field
            label="Git executable path"
            htmlFor="git-path"
            hint="Full path to git, or just “git” if it is on PATH."
          >
            <div className="flex gap-2">
              <Input
                id="git-path"
                value={draft?.git_path ?? ''}
                onChange={(e) => patch({ git_path: e.target.value })}
                placeholder="git"
              />
              <Button onClick={testGit} disabled={testing}>
                {testing ? 'Testing…' : 'Test'}
              </Button>
            </div>
            {gitPath && (
              <p
                className={`mt-2 flex items-center gap-1.5 text-xs ${gitPath.ok ? 'text-success' : 'text-error'}`}
              >
                {gitPath.ok ? (
                  <CheckCircle2 size={13} aria-hidden />
                ) : (
                  <XCircle size={13} aria-hidden />
                )}
                {gitPath.msg}
              </p>
            )}
          </Field>

          <Field
            label="GitHub OAuth client id"
            htmlFor="oauth-client-id"
            hint="Register your own OAuth app (GitHub → Settings → Developer settings → OAuth apps) and paste its client id here. Device flow needs the client id only — never a client secret. The token stays on this machine."
          >
            <Input
              id="oauth-client-id"
              value={draft?.github_client_id ?? ''}
              onChange={(e) => patch({ github_client_id: e.target.value })}
              placeholder="Iv1.abc123…"
              autoComplete="off"
            />
          </Field>
        </div>
      </Card>

      {/* App behaviour */}
      <Card>
        <CardHeader title="Application" description="Logging, notifications and language" />
        <div className="grid grid-cols-1 gap-4 px-5 py-4 md:grid-cols-2">
          <Field label="Logging level" htmlFor="log-level" hint="Verbose levels produce larger log files.">
            <Select
              id="log-level"
              value={draft?.log_level ?? 'info'}
              onChange={(e) => patch({ log_level: e.target.value })}
            >
              <option value="debug">debug</option>
              <option value="info">info</option>
              <option value="warn">warn</option>
              <option value="error">error</option>
            </Select>
          </Field>

          <Field label="Language" htmlFor="language" hint="More languages are not available yet.">
            <Select id="language" defaultValue="en" disabled>
              <option value="en">English</option>
            </Select>
          </Field>

          <div className="flex items-center justify-between rounded-lg border border-border bg-surface px-4 py-3">
            <div>
              <p className="text-sm font-medium text-foreground">Desktop notifications</p>
              <p className="text-xs text-muted">Notify when a backup or publish finishes.</p>
            </div>
            <Toggle
              checked={draft?.notifications ?? true}
              onChange={(v) => patch({ notifications: v })}
              label="Desktop notifications"
            />
          </div>

          <div className="flex items-center justify-between rounded-lg border border-border bg-surface px-4 py-3">
            <div>
              <p className="text-sm font-medium text-foreground">App version</p>
              <p className="text-xs text-muted">Running build of Git Backup Restore.</p>
            </div>
            <span className="font-mono text-sm font-medium text-foreground">
              {version || '—'}
            </span>
          </div>
        </div>
      </Card>

      {/* Save bar */}
      <div className="sticky bottom-4 z-10 flex flex-wrap items-center justify-between gap-3 rounded-xl border border-border bg-white px-4 py-3 shadow-lift">
        <p className="text-sm text-secondary">
          {saved ? (
            <span className="flex items-center gap-1.5 text-success">
              <CheckCircle2 size={14} aria-hidden />
              Settings saved.
            </span>
          ) : (
            'Changes are not applied until you save.'
          )}
        </p>
        <div className="flex items-center gap-2">
          <Button onClick={reload} disabled={saving}>
            Reload
          </Button>
          <Button variant="primary" onClick={save} disabled={saving || !draft || !!error}>
            <Save size={14} aria-hidden />
            {saving ? 'Saving…' : 'Save settings'}
          </Button>
        </div>
      </div>

      <Notice tone="info" title="Where is my data?">
        Tokens, settings and backups never leave this machine unless you explicitly publish a
        repository to GitHub.
      </Notice>
    </div>
  );
}
