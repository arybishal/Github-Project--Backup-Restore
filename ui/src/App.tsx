import {
  FolderGit2,
  FolderOpen,
  HardDrive,
  History,
  Info,
  LayoutDashboard,
  ListChecks,
  RotateCcw,
  Save,
  ScrollText,
  Settings as SettingsIcon,
  UploadCloud,
  type LucideIcon,
} from 'lucide-react';
import { useCallback, useEffect, useState, type ReactNode } from 'react';
import { isTauri } from './api/client';
import { Notice, cn } from './components/ui';
import { useMediaQuery } from './lib/hooks';
import { NavContext, type PageId } from './store/nav';
import About from './pages/About';
import ActivityLogs from './pages/ActivityLogs';
import Backup from './pages/Backup';
import BackupHistory from './pages/BackupHistory';
import Dashboard from './pages/Dashboard';
import GitHubPublisher from './pages/GitHubPublisher';
import LocalProjects from './pages/LocalProjects';
import Repositories from './pages/Repositories';
import Restore from './pages/Restore';
import Settings from './pages/Settings';
import StorageManager from './pages/StorageManager';
import TaskCenter from './pages/TaskCenter';

const NAV_ITEMS: { id: PageId; label: string; icon: LucideIcon }[] = [
  { id: 'dashboard', label: 'Dashboard', icon: LayoutDashboard },
  { id: 'repositories', label: 'Repositories', icon: FolderGit2 },
  { id: 'backup', label: 'Backup', icon: Save },
  { id: 'restore', label: 'Restore', icon: RotateCcw },
  { id: 'history', label: 'Backup History', icon: History },
  { id: 'local-projects', label: 'Local Projects', icon: FolderOpen },
  { id: 'publisher', label: 'GitHub Publisher', icon: UploadCloud },
  { id: 'storage', label: 'Storage Manager', icon: HardDrive },
  { id: 'tasks', label: 'Task Center', icon: ListChecks },
  { id: 'logs', label: 'Activity Logs', icon: ScrollText },
  { id: 'settings', label: 'Settings', icon: SettingsIcon },
  { id: 'about', label: 'About', icon: Info },
];

const VALID_PAGES = new Set<string>(NAV_ITEMS.map((i) => i.id));

function pageFromHash(): PageId {
  const raw = window.location.hash.replace(/^#\/?/, '');
  return VALID_PAGES.has(raw) ? (raw as PageId) : 'dashboard';
}

function renderPage(page: PageId): ReactNode {
  switch (page) {
    case 'dashboard':
      return <Dashboard />;
    case 'repositories':
      return <Repositories />;
    case 'backup':
      return <Backup />;
    case 'restore':
      return <Restore />;
    case 'history':
      return <BackupHistory />;
    case 'local-projects':
      return <LocalProjects />;
    case 'publisher':
      return <GitHubPublisher />;
    case 'storage':
      return <StorageManager />;
    case 'tasks':
      return <TaskCenter />;
    case 'logs':
      return <ActivityLogs />;
    case 'settings':
      return <Settings />;
    case 'about':
      return <About />;
  }
}

export default function App() {
  const [page, setPage] = useState<PageId>(() => pageFromHash());

  useEffect(() => {
    const onHash = () => setPage(pageFromHash());
    window.addEventListener('hashchange', onHash);
    return () => window.removeEventListener('hashchange', onHash);
  }, []);

  const navigate = useCallback((next: PageId) => {
    if (window.location.hash !== `#${next}`) {
      window.location.hash = next;
    }
    setPage(next);
  }, []);

  // Sidebar collapses to 64px below 900px so the layout never breaks.
  const narrow = useMediaQuery('(max-width: 900px)');
  const title = NAV_ITEMS.find((i) => i.id === page)?.label ?? 'Dashboard';

  return (
    <NavContext.Provider value={{ page, navigate }}>
      <div className="min-h-screen bg-background text-foreground">
        <aside
          className={cn(
            'fixed inset-y-0 left-0 z-40 flex flex-col border-r border-border bg-white transition-all duration-150',
            narrow ? 'w-16' : 'w-60',
          )}
          aria-label="Main navigation"
        >
          <div
            className={cn(
              'flex h-14 shrink-0 items-center gap-2.5 border-b border-border px-4',
              narrow && 'justify-center px-0',
            )}
          >
            <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-primary">
              <FolderGit2 size={16} className="text-foreground" aria-hidden />
            </div>
            {!narrow && (
              <span className="truncate text-sm font-semibold text-foreground">
                Git Backup Restore
              </span>
            )}
          </div>

          <nav className="flex-1 space-y-1 overflow-y-auto px-2 py-3">
            {NAV_ITEMS.map((item) => {
              const active = page === item.id;
              return (
                <button
                  key={item.id}
                  type="button"
                  onClick={() => navigate(item.id)}
                  aria-current={active ? 'page' : undefined}
                  title={narrow ? item.label : undefined}
                  className={cn(
                    'relative flex w-full items-center gap-3 rounded-lg px-3 py-2.5 text-sm font-medium transition-all duration-150',
                    active
                      ? 'bg-pale text-foreground'
                      : 'text-secondary hover:bg-surface hover:text-foreground',
                    narrow && 'justify-center px-0',
                  )}
                >
                  {active && (
                    <span
                      className="absolute left-0 top-1/2 h-5 w-1 -translate-y-1/2 rounded-r-full bg-primary"
                      aria-hidden
                    />
                  )}
                  <item.icon size={18} className="shrink-0" aria-hidden />
                  {!narrow && <span className="truncate">{item.label}</span>}
                </button>
              );
            })}
          </nav>
        </aside>

        <div className={cn('min-h-screen transition-all duration-150', narrow ? 'pl-16' : 'pl-60')}>
          <header className="sticky top-0 z-30 flex h-14 items-center justify-between gap-4 border-b border-border bg-background px-6">
            <h1 className="truncate text-base font-semibold text-foreground">{title}</h1>
            <span
              className={cn(
                'inline-flex shrink-0 items-center gap-1.5 rounded-full border px-2.5 py-1 text-[11px] font-medium',
                isTauri
                  ? 'border-[#DCFCE7] bg-[#F0FDF4] text-success'
                  : 'border-[#FDE68A] bg-[#FFFBEB] text-warning',
              )}
            >
              <span
                className={cn('h-1.5 w-1.5 rounded-full', isTauri ? 'bg-success' : 'bg-warning')}
                aria-hidden
              />
              {isTauri ? 'Desktop backend connected' : 'Backend unavailable'}
            </span>
          </header>

          <main className="mx-auto w-full max-w-[1440px] px-6 py-6">
            {!isTauri && (
              <Notice tone="warning" title="Desktop backend not available" className="mb-5">
                You are viewing the UI in a plain browser tab. Live data comes from the Tauri
                desktop backend — open the app window to connect. Every screen below will report
                this instead of showing placeholder data.
              </Notice>
            )}
            {renderPage(page)}
          </main>
        </div>
      </div>
    </NavContext.Provider>
  );
}
