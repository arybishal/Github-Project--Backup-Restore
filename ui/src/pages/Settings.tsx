import {
  ArrowUpCircle,
  CheckCircle2,
  Copy,
  Download,
  ExternalLink,
  KeyRound,
  LogOut,
  MonitorSmartphone,
  RefreshCw,
  Save,
  ScrollText,
  ShieldCheck,
  Sparkles,
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
  check_for_updates,
  git_test,
  install_update,
  logs_export,
  logs_list,
  open_external_url,
  pick_directory,
  settings_get,
  settings_set,
} from '../api/client';
import type {
  AccountInfo,
  DeviceCode,
  LogEntry,
  Settings as SettingsType,
  UpdateInfo,
} from '../api/types';
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
import { useNav } from '../store/nav';

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

  const { navigate } = useNav();

  // Updates state
  const [checkingUpdate, setCheckingUpdate] = useState(false);
  const [updateInfo, setUpdateInfo] = useState<UpdateInfo | null>(null);
  const [updateChecked, setUpdateChecked] = useState(false);
  const [updateError, setUpdateError] = useState<string | null>(null);
  const [installingUpdate, setInstallingUpdate] = useState(false);
  const [installStatus, setInstallStatus] = useState<string | null>(null);

  // Settings Logs state
  const [settingsLogLevel, setSettingsLogLevel] = useState<string>('all');
  const [settingsLogs, setSettingsLogs] = useState<LogEntry[]>([]);
  const [logsLoading, setLogsLoading] = useState(false);
  const [logsCopied, setLogsCopied] = useState(false);
  const [exportingLogs, setExportingLogs] = useState(false);
  const [exportNotice, setExportNotice] = useState<string | null>(null);

  async function checkForUpdates() {
    setCheckingUpdate(true);
    setUpdateError(null);
    setInstallStatus(null);
    try {
      const res = await check_for_updates();
      setUpdateInfo(res);
      setUpdateChecked(true);
    } catch (e) {
      setUpdateError(errMsg(e));
    } finally {
      setCheckingUpdate(false);
    }
  }

  async function handleInstallUpdate() {
    if (!updateInfo?.download_url || !updateInfo?.asset_name) return;
    setInstallingUpdate(true);
    setUpdateError(null);
    try {
      const msg = await install_update({
        downloadUrl: updateInfo.download_url,
        assetName: updateInfo.asset_name,
      });
      setInstallStatus(msg);
    } catch (e) {
      setUpdateError(errMsg(e));
    } finally {
      setInstallingUpdate(false);
    }
  }

  async function handleOpenReleaseUrl(url: string) {
    try {
      await open_external_url(url);
    } catch {
      window.open(url, '_blank');
    }
  }

  async function loadSettingsLogs() {
    setLogsLoading(true);
    try {
      const list = await logs_list({
        level: settingsLogLevel === 'all' ? '' : settingsLogLevel,
        search: '',
        limit: 30,
      });
      setSettingsLogs(list);
    } catch {
      // quiet fallback
    } finally {
      setLogsLoading(false);
    }
  }

  useEffect(() => {
    loadSettingsLogs();
  }, [settingsLogLevel]);

  async function copySettingsLogs() {
    try {
      const text = settingsLogs
        .map(
          (l) =>
            `[${l.ts}] ${l.level.toUpperCase()} [${l.source}]: ${l.message}${
              l.details ? ` | ${l.details}` : ''
            }`,
        )
        .join('\n');
      await navigator.clipboard.writeText(text);
      setLogsCopied(true);
      setTimeout(() => setLogsCopied(false), 2000);
    } catch {
      // ignore
    }
  }

  async function exportSettingsLogs() {
    setExportingLogs(true);
    setExportNotice(null);
    try {
      const path = await logs_export();
      setExportNotice(`Logs exported to: ${path}`);
      setTimeout(() => setExportNotice(null), 5000);
    } catch (e) {
      setExportNotice(`Export failed: ${errMsg(e)}`);
    } finally {
      setExportingLogs(false);
    }
  }

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

      {/* Software Updates */}
      <Card>
        <CardHeader
          title="Software Updates"
          description="Check for new releases, changelogs and update directly"
          actions={
            <Button onClick={checkForUpdates} disabled={checkingUpdate}>
              <RefreshCw
                size={13}
                className={checkingUpdate ? 'animate-spin' : ''}
                aria-hidden
              />
              {checkingUpdate ? 'Checking GitHub…' : 'Check for Updates'}
            </Button>
          }
        />
        <div className="space-y-4 px-5 py-4">
          <div className="flex flex-wrap items-center justify-between gap-4 rounded-lg border border-border bg-surface p-4">
            <div>
              <p className="text-sm font-medium text-foreground">Current Installed Version</p>
              <p className="text-xs text-muted">Git Backup Restore desktop engine.</p>
            </div>
            <div className="flex items-center gap-2">
              <Badge tone="neutral">v{version || '0.1.0'}</Badge>
              {updateChecked && !updateInfo?.update_available && (
                <Badge tone="success">
                  <CheckCircle2 size={11} aria-hidden />
                  Latest Version
                </Badge>
              )}
            </div>
          </div>

          {updateError && (
            <p className="text-xs text-error">Failed to check updates: {updateError}</p>
          )}

          {installStatus && (
            <Notice tone="info" title="Update Status">
              {installStatus}
            </Notice>
          )}

          {updateChecked && updateInfo?.update_available && (
            <div className="rounded-xl border-2 border-primary bg-amber-500/10 p-4">
              <div className="flex flex-wrap items-start justify-between gap-3">
                <div>
                  <div className="flex items-center gap-2">
                    <Sparkles size={16} className="text-foreground" />
                    <h4 className="text-sm font-bold text-foreground">
                      New Release Available: v{updateInfo.latest_version}
                    </h4>
                    <Badge tone="yellow">Update Ready</Badge>
                  </div>
                  <p className="mt-1 text-xs text-secondary">
                    {updateInfo.release_name}
                    {updateInfo.published_at ? ` · Published on ${updateInfo.published_at.slice(0, 10)}` : ''}
                  </p>
                </div>
                <div className="flex flex-wrap items-center gap-2">
                  <Button
                    onClick={() => handleOpenReleaseUrl(updateInfo.release_url)}
                  >
                    <ExternalLink size={13} aria-hidden />
                    View Changelog
                  </Button>
                  {updateInfo.download_url && (
                    <Button
                      variant="primary"
                      onClick={handleInstallUpdate}
                      disabled={installingUpdate}
                    >
                      <ArrowUpCircle size={14} aria-hidden />
                      {installingUpdate ? 'Downloading & Installing…' : 'Update Directly'}
                    </Button>
                  )}
                </div>
              </div>
              {updateInfo.release_notes && (
                <div className="mt-3 rounded-lg border border-border bg-white p-3 font-mono text-xs text-foreground">
                  <p className="font-semibold text-secondary mb-1">Release Highlights:</p>
                  <pre className="max-h-36 overflow-y-auto whitespace-pre-wrap font-sans text-xs text-muted">
                    {updateInfo.release_notes.slice(0, 500)}
                    {updateInfo.release_notes.length > 500 ? '…' : ''}
                  </pre>
                </div>
              )}
            </div>
          )}
        </div>
      </Card>

      {/* Activity & Diagnostic Logs in Settings */}
      <Card>
        <CardHeader
          title="Activity & Diagnostic Logs"
          description="View recent system activity, backup logs, and diagnostic events directly in Settings"
          actions={
            <div className="flex flex-wrap items-center gap-2">
              <Button onClick={copySettingsLogs} disabled={settingsLogs.length === 0}>
                <Copy size={13} aria-hidden />
                {logsCopied ? 'Copied!' : 'Copy'}
              </Button>
              <Button onClick={exportSettingsLogs} disabled={exportingLogs}>
                <Download size={13} aria-hidden />
                {exportingLogs ? 'Exporting…' : 'Export (.txt)'}
              </Button>
              <Button variant="primary" onClick={() => navigate('logs')}>
                <ScrollText size={13} aria-hidden />
                Full Logs Page
              </Button>
            </div>
          }
        />
        <div className="space-y-4 px-5 py-4">
          <div className="flex flex-wrap items-center justify-between gap-3">
            <div className="flex items-center gap-2">
              <label htmlFor="settings-log-filter" className="text-xs font-medium text-muted">
                Filter by level:
              </label>
              <Select
                id="settings-log-filter"
                value={settingsLogLevel}
                onChange={(e) => setSettingsLogLevel(e.target.value)}
                className="w-32 py-1 text-xs"
              >
                <option value="all">All levels</option>
                <option value="error">Error</option>
                <option value="warn">Warn</option>
                <option value="info">Info</option>
                <option value="debug">Debug</option>
              </Select>
            </div>
            <Button onClick={loadSettingsLogs} disabled={logsLoading}>
              <RefreshCw size={12} className={logsLoading ? 'animate-spin' : ''} aria-hidden />
              Refresh
            </Button>
          </div>

          {exportNotice && (
            <p className="text-xs text-success">{exportNotice}</p>
          )}

          <div className="rounded-lg border border-border bg-[#111111] p-3 font-mono text-xs text-[#FFFFFF]">
            <div className="max-h-64 overflow-y-auto space-y-2 scrollbar-thin pr-1">
              {logsLoading && settingsLogs.length === 0 ? (
                <p className="py-6 text-center text-xs text-[#888888]">Loading activity logs…</p>
              ) : settingsLogs.length === 0 ? (
                <p className="py-6 text-center text-xs text-[#888888]">No log entries matching this level.</p>
              ) : (
                settingsLogs.map((log) => (
                  <div key={log.id} className="flex items-start gap-2.5 leading-relaxed border-b border-[#222222] pb-1.5 last:border-0 last:pb-0">
                    <span className="shrink-0 text-[11px] text-[#888888]">
                      {log.ts.length >= 19 ? log.ts.slice(11, 19) : log.ts}
                    </span>
                    <span
                      className={`shrink-0 rounded px-1.5 py-0.5 text-[9px] font-bold uppercase ${
                        log.level === 'error'
                          ? 'border border-red-700 bg-red-950 text-red-300'
                          : log.level === 'warn' || log.level === 'warning'
                          ? 'border border-[#F5C400] bg-amber-950 text-[#F5C400]'
                          : 'bg-[#262626] text-[#D4D4D4]'
                      }`}
                    >
                      {log.level}
                    </span>
                    <span className="shrink-0 text-[11px] font-semibold text-[#F5C400]">
                      [{log.source}]
                    </span>
                    <span className="min-w-0 flex-1 break-words text-[#EAEAEA]">
                      {log.message}
                      {log.details && (
                        <span className="block mt-0.5 text-[11px] text-[#999999]">{log.details}</span>
                      )}
                    </span>
                  </div>
                ))
              )}
            </div>
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
