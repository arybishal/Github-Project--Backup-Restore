import { FolderGit2, RefreshCw, Search } from 'lucide-react';
import { useEffect, useMemo, useState } from 'react';
import { repos_list, repos_sync } from '../api/client';
import {
  Badge,
  Button,
  Card,
  Checkbox,
  EmptyState,
  ErrorBanner,
  Input,
  PageLoading,
  Select,
  Table,
  TD,
  TH,
  TR,
} from '../components/ui';
import { errMsg, formatBytes, formatDate } from '../lib/format';
import { useAsync } from '../lib/hooks';
import { useNav } from '../store/nav';
import { setSelectedRepos, useSelectedRepos } from '../store/selection';

const PAGE_SIZE = 50;

const LANG_COLORS: Record<string, string> = {
  TypeScript: '#3178C6',
  JavaScript: '#F7DF1E',
  Python: '#3572A5',
  Rust: '#DEA584',
  Go: '#00ADD8',
  Java: '#B07219',
  'C#': '#178600',
  'C++': '#F34B7D',
  PHP: '#4F5D95',
  Ruby: '#701516',
  Swift: '#F05138',
  Kotlin: '#A97BFF',
  Vue: '#41B883',
  HTML: '#E34C26',
  CSS: '#563D7C',
  Shell: '#89E051',
  Dart: '#00B4AB',
  Lua: '#000080',
};

function backupBadge(status: string | null) {
  if (!status) return <span className="text-xs text-muted">Never</span>;
  const s = status.toLowerCase();
  if (['succeeded', 'success', 'ok', 'verified'].includes(s)) {
    return <Badge tone="success">{status}</Badge>;
  }
  if (['failed', 'error'].includes(s)) return <Badge tone="error">{status}</Badge>;
  return <Badge tone="warning">{status}</Badge>;
}

type Visibility = 'all' | 'public' | 'private';
type Archived = 'all' | 'archived' | 'active';
type Sort = 'name' | 'updated' | 'size';

export default function Repositories() {
  const { navigate } = useNav();
  const { data: repos, error, loading, reload } = useAsync(() => repos_list({ filter: {} }), []);
  const selected = useSelectedRepos();

  const [search, setSearch] = useState('');
  const [owner, setOwner] = useState('all');
  const [language, setLanguage] = useState('all');
  const [visibility, setVisibility] = useState<Visibility>('all');
  const [archived, setArchived] = useState<Archived>('all');
  const [sort, setSort] = useState<Sort>('updated');
  const [page, setPage] = useState(1);
  const [syncing, setSyncing] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);

  useEffect(() => {
    setPage(1);
  }, [search, owner, language, visibility, archived, sort]);

  const owners = useMemo(() => {
    const set = new Set<string>();
    (repos ?? []).forEach((r) => set.add(r.owner));
    return [...set].sort((a, b) => a.localeCompare(b));
  }, [repos]);

  const languages = useMemo(() => {
    const set = new Set<string>();
    (repos ?? []).forEach((r) => {
      if (r.language) set.add(r.language);
    });
    return [...set].sort((a, b) => a.localeCompare(b));
  }, [repos]);

  const filtered = useMemo(() => {
    const q = search.trim().toLowerCase();
    const list = (repos ?? []).filter((r) => {
      if (
        q &&
        !r.full_name.toLowerCase().includes(q) &&
        !(r.description ?? '').toLowerCase().includes(q)
      ) {
        return false;
      }
      if (owner !== 'all' && r.owner !== owner) return false;
      if (language !== 'all' && r.language !== language) return false;
      if (visibility === 'public' && r.is_private) return false;
      if (visibility === 'private' && !r.is_private) return false;
      if (archived === 'archived' && !r.is_archived) return false;
      if (archived === 'active' && r.is_archived) return false;
      return true;
    });
    const sorted = [...list];
    if (sort === 'name') {
      sorted.sort((a, b) => a.full_name.localeCompare(b.full_name));
    } else if (sort === 'updated') {
      sorted.sort((a, b) => (b.updated_at ?? '').localeCompare(a.updated_at ?? ''));
    } else {
      sorted.sort((a, b) => (b.size_kb ?? 0) - (a.size_kb ?? 0));
    }
    return sorted;
  }, [repos, search, owner, language, visibility, archived, sort]);

  const totalPages = Math.max(1, Math.ceil(filtered.length / PAGE_SIZE));
  const currentPage = Math.min(page, totalPages);
  const pageRows = filtered.slice((currentPage - 1) * PAGE_SIZE, currentPage * PAGE_SIZE);

  const selectedSet = useMemo(() => new Set(selected), [selected]);
  const pageAllOn = pageRows.length > 0 && pageRows.every((r) => selectedSet.has(r.full_name));

  function toggleRow(name: string) {
    if (selectedSet.has(name)) {
      setSelectedRepos(selected.filter((n) => n !== name));
    } else {
      setSelectedRepos([...selected, name]);
    }
  }

  function selectMany(names: string[], on: boolean) {
    const set = new Set(selected);
    names.forEach((n) => (on ? set.add(n) : set.delete(n)));
    setSelectedRepos([...set]);
  }

  function togglePage() {
    selectMany(
      pageRows.map((r) => r.full_name),
      !pageAllOn,
    );
  }

  function clearFilters() {
    setSearch('');
    setOwner('all');
    setLanguage('all');
    setVisibility('all');
    setArchived('all');
    setSort('updated');
  }

  async function refresh() {
    setSyncing(true);
    setActionError(null);
    try {
      await repos_sync();
      reload();
    } catch (e) {
      setActionError(errMsg(e));
    } finally {
      setSyncing(false);
    }
  }

  if (loading && !repos) return <PageLoading rows={8} />;

  const hasAny = (repos ?? []).length > 0;

  return (
    <div className="space-y-4">
      <ErrorBanner error={error ?? actionError} onRetry={error ? reload : undefined} />

      {!error && hasAny && (
        <Card className="p-4">
          <div className="flex flex-col gap-3">
            {/* Row 1: search + refresh */}
            <div className="flex flex-wrap items-center gap-2">
              <div className="relative min-w-[220px] flex-1">
                <Search
                  size={14}
                  className="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-muted"
                  aria-hidden
                />
                <Input
                  value={search}
                  onChange={(e) => setSearch(e.target.value)}
                  placeholder="Search by name or description…"
                  aria-label="Search repositories"
                  className="pl-8"
                />
              </div>
              <Button onClick={refresh} disabled={syncing}>
                <RefreshCw size={14} className={syncing ? 'animate-spin' : ''} aria-hidden />
                {syncing ? 'Syncing…' : 'Refresh'}
              </Button>
            </div>

            {/* Row 2: filters */}
            <div className="flex flex-wrap items-end gap-2">
              <label className="flex flex-col gap-1 text-xs font-medium text-muted">
                Owner
                <Select
                  value={owner}
                  onChange={(e) => setOwner(e.target.value)}
                  aria-label="Filter by owner"
                  className="w-40"
                >
                  <option value="all">All owners</option>
                  {owners.map((o) => (
                    <option key={o} value={o}>
                      {o}
                    </option>
                  ))}
                </Select>
              </label>
              <label className="flex flex-col gap-1 text-xs font-medium text-muted">
                Language
                <Select
                  value={language}
                  onChange={(e) => setLanguage(e.target.value)}
                  aria-label="Filter by language"
                  className="w-40"
                >
                  <option value="all">All languages</option>
                  {languages.map((l) => (
                    <option key={l} value={l}>
                      {l}
                    </option>
                  ))}
                </Select>
              </label>
              <label className="flex flex-col gap-1 text-xs font-medium text-muted">
                Visibility
                <Select
                  value={visibility}
                  onChange={(e) => setVisibility(e.target.value as Visibility)}
                  aria-label="Filter by visibility"
                  className="w-36"
                >
                  <option value="all">All</option>
                  <option value="public">Public</option>
                  <option value="private">Private</option>
                </Select>
              </label>
              <label className="flex flex-col gap-1 text-xs font-medium text-muted">
                Archived
                <Select
                  value={archived}
                  onChange={(e) => setArchived(e.target.value as Archived)}
                  aria-label="Filter by archived state"
                  className="w-36"
                >
                  <option value="all">All</option>
                  <option value="active">Active only</option>
                  <option value="archived">Archived only</option>
                </Select>
              </label>
              <label className="flex flex-col gap-1 text-xs font-medium text-muted">
                Sort by
                <Select
                  value={sort}
                  onChange={(e) => setSort(e.target.value as Sort)}
                  aria-label="Sort repositories"
                  className="w-36"
                >
                  <option value="updated">Last updated</option>
                  <option value="name">Name</option>
                  <option value="size">Size</option>
                </Select>
              </label>
            </div>

            {/* Row 3: bulk selection */}
            <div className="flex flex-wrap items-center gap-2 border-t border-border pt-3">
              <span className="text-xs font-medium text-muted">Select:</span>
              <Button size="sm" variant="ghost" onClick={() => selectMany(filtered.map((r) => r.full_name), true)}>
                All
              </Button>
              <Button size="sm" variant="ghost" onClick={() => setSelectedRepos([])}>
                None
              </Button>
              <Button
                size="sm"
                variant="ghost"
                onClick={() =>
                  selectMany(
                    filtered.filter((r) => r.is_private).map((r) => r.full_name),
                    true,
                  )
                }
              >
                Private
              </Button>
              <Button
                size="sm"
                variant="ghost"
                onClick={() =>
                  selectMany(
                    filtered.filter((r) => !r.is_private).map((r) => r.full_name),
                    true,
                  )
                }
              >
                Public
              </Button>
              <span className="ml-auto text-xs text-muted">
                {filtered.length} of {repos?.length ?? 0} repositories shown
              </span>
            </div>
          </div>
        </Card>
      )}

      {!error && !hasAny && !loading && (
        <EmptyState
          icon={FolderGit2}
          title="No repositories synced yet"
          description="Connect your GitHub account in Settings, then press Refresh to pull your repository list. Nothing is shown until real data arrives from the backend."
          action={
            <div className="flex gap-2">
              <Button variant="primary" onClick={refresh} disabled={syncing}>
                <RefreshCw size={14} aria-hidden />
                Sync now
              </Button>
              <Button onClick={() => navigate('settings')}>Open Settings</Button>
            </div>
          }
        />
      )}

      {!error && hasAny && filtered.length === 0 && (
        <EmptyState
          icon={Search}
          title="No repositories match your filters"
          description="Try a different search term, or clear the filters to see every synced repository."
          action={<Button onClick={clearFilters}>Clear filters</Button>}
        />
      )}

      {!error && hasAny && filtered.length > 0 && (
        <>
          <Table>
            <thead>
              <tr>
                <TH className="w-10">
                  <Checkbox
                    checked={pageAllOn}
                    onChange={togglePage}
                    aria-label="Select all repositories on this page"
                  />
                </TH>
                <TH>Repository</TH>
                <TH className="hidden lg:table-cell">Description</TH>
                <TH>Visibility</TH>
                <TH className="hidden xl:table-cell">Language</TH>
                <TH className="hidden xl:table-cell">Branch</TH>
                <TH>Updated</TH>
                <TH className="hidden xl:table-cell">Size</TH>
                <TH>Last backup</TH>
              </tr>
            </thead>
            <tbody>
              {pageRows.map((r) => {
                const isSelected = selectedSet.has(r.full_name);
                return (
                  <TR key={r.github_id} selected={isSelected}>
                    <TD>
                      <Checkbox
                        checked={isSelected}
                        onChange={() => toggleRow(r.full_name)}
                        aria-label={`Select ${r.full_name}`}
                      />
                    </TD>
                    <TD>
                      <div className="flex items-center gap-2">
                        <div className="min-w-0">
                          <p className="truncate text-sm font-medium text-foreground">{r.name}</p>
                          <p className="truncate text-xs text-muted">{r.owner}</p>
                        </div>
                        {r.is_fork && <Badge>Fork</Badge>}
                        {r.is_archived && <Badge tone="warning">Archived</Badge>}
                      </div>
                    </TD>
                    <TD className="hidden max-w-[260px] lg:table-cell">
                      <span className="block truncate" title={r.description ?? ''}>
                        {r.description ?? '—'}
                      </span>
                    </TD>
                    <TD>
                      <Badge tone={r.is_private ? 'dark' : 'neutral'}>
                        {r.is_private ? 'Private' : 'Public'}
                      </Badge>
                    </TD>
                    <TD className="hidden xl:table-cell">
                      {r.language ? (
                        <span className="flex items-center gap-1.5 text-xs">
                          <span
                            className="inline-block h-2 w-2 rounded-full"
                            style={{ backgroundColor: LANG_COLORS[r.language] ?? '#B0B0B0' }}
                            aria-hidden
                          />
                          {r.language}
                        </span>
                      ) : (
                        <span className="text-xs text-muted">—</span>
                      )}
                    </TD>
                    <TD className="hidden font-mono text-xs xl:table-cell">
                      {r.default_branch ?? '—'}
                    </TD>
                    <TD className="whitespace-nowrap text-xs">{formatDate(r.updated_at)}</TD>
                    <TD className="hidden whitespace-nowrap text-xs xl:table-cell">
                      {r.size_kb != null ? formatBytes(r.size_kb * 1024) : '—'}
                    </TD>
                    <TD>{backupBadge(r.last_backup_status)}</TD>
                  </TR>
                );
              })}
            </tbody>
          </Table>

          {/* Pagination */}
          <div className="flex flex-wrap items-center justify-between gap-3">
            <p className="text-xs text-muted">
              Showing {(currentPage - 1) * PAGE_SIZE + 1}–
              {Math.min(currentPage * PAGE_SIZE, filtered.length)} of {filtered.length}
            </p>
            <div className="flex items-center gap-2">
              <Button
                size="sm"
                disabled={currentPage <= 1}
                onClick={() => setPage(currentPage - 1)}
              >
                Previous
              </Button>
              <span className="text-xs text-secondary">
                Page {currentPage} of {totalPages}
              </span>
              <Button
                size="sm"
                disabled={currentPage >= totalPages}
                onClick={() => setPage(currentPage + 1)}
              >
                Next
              </Button>
            </div>
          </div>
        </>
      )}

      {/* Selection footer */}
      <div className="sticky bottom-4 z-10 flex flex-wrap items-center justify-between gap-3 rounded-xl border border-border bg-white px-4 py-3 shadow-lift">
        <p className="text-sm text-secondary">
          <span className="font-semibold text-foreground">{selected.length}</span>{' '}
          {selected.length === 1 ? 'repository' : 'repositories'} selected
          {selected.length > 0 && (
            <span className="ml-2 text-xs text-muted">
              {selected.slice(0, 3).join(', ')}
              {selected.length > 3 ? ` +${selected.length - 3} more` : ''}
            </span>
          )}
        </p>
        <div className="flex items-center gap-2">
          <Button size="sm" variant="ghost" onClick={() => setSelectedRepos([])} disabled={selected.length === 0}>
            Clear
          </Button>
          <Button
            size="sm"
            variant="primary"
            disabled={selected.length === 0}
            onClick={() => navigate('backup')}
          >
            Backup selected
          </Button>
        </div>
      </div>
    </div>
  );
}
