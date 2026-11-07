// Hand-written UI primitives in the shadcn visual style (no component library):
// Button, Card, Input, Select, Checkbox, Toggle, Badge, Tabs, Table, Progress,
// Dialog, EmptyState, Skeleton, ErrorBanner, StatCard.
import {
  AlertCircle,
  ChevronDown,
  Inbox,
  Loader2,
  X,
  type LucideIcon,
} from 'lucide-react';
import {
  useEffect,
  useRef,
  type ButtonHTMLAttributes,
  type HTMLAttributes,
  type InputHTMLAttributes,
  type ReactNode,
  type SelectHTMLAttributes,
} from 'react';

export function cn(...parts: (string | false | null | undefined)[]): string {
  return parts.filter(Boolean).join(' ');
}

/* ------------------------------- Button ---------------------------------- */

type ButtonVariant = 'primary' | 'outline' | 'ghost' | 'danger';
type ButtonSize = 'sm' | 'default' | 'lg';

const BUTTON_VARIANTS: Record<ButtonVariant, string> = {
  primary: 'bg-primary text-foreground hover:bg-primary-soft border border-transparent',
  outline:
    'bg-white text-foreground border border-border hover:bg-surface',
  ghost: 'bg-transparent text-secondary border border-transparent hover:bg-surface hover:text-foreground',
  danger: 'bg-error text-white border border-transparent hover:opacity-90',
};

const BUTTON_SIZES: Record<ButtonSize, string> = {
  sm: 'h-8 px-3 text-xs rounded-lg gap-1.5',
  default: 'h-9 px-4 text-sm rounded-lg gap-2',
  lg: 'h-11 px-5 text-sm rounded-lg gap-2',
};

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant;
  size?: ButtonSize;
}

export function Button({
  variant = 'outline',
  size = 'default',
  className,
  type = 'button',
  ...props
}: ButtonProps) {
  return (
    <button
      type={type}
      className={cn(
        'inline-flex select-none items-center justify-center font-medium transition-all duration-150',
        'disabled:pointer-events-none disabled:opacity-50',
        BUTTON_VARIANTS[variant],
        BUTTON_SIZES[size],
        className,
      )}
      {...props}
    />
  );
}

/* -------------------------------- Card ----------------------------------- */

export function Card({
  className,
  children,
  ...props
}: { className?: string; children: ReactNode } & HTMLAttributes<HTMLDivElement>) {
  return (
    <div
      className={cn('rounded-xl border border-border bg-white shadow-soft', className)}
      {...props}
    >
      {children}
    </div>
  );
}

export function CardHeader({
  title,
  description,
  actions,
}: {
  title: string;
  description?: string;
  actions?: ReactNode;
}) {
  return (
    <div className="flex flex-wrap items-start justify-between gap-3 border-b border-border px-5 py-4">
      <div className="min-w-0">
        <h2 className="text-sm font-semibold text-foreground">{title}</h2>
        {description && <p className="mt-0.5 text-xs text-muted">{description}</p>}
      </div>
      {actions && <div className="flex shrink-0 items-center gap-2">{actions}</div>}
    </div>
  );
}

/* ------------------------------- Inputs ---------------------------------- */

export interface InputProps extends InputHTMLAttributes<HTMLInputElement> {}

export function Input({ className, ...props }: InputProps) {
  return (
    <input
      className={cn(
        'h-9 w-full rounded-lg border border-border bg-white px-3 text-sm text-foreground',
        'placeholder:text-muted transition-colors duration-150',
        'focus:outline-none focus:ring-2 focus:ring-primary focus:border-transparent',
        'disabled:bg-surface disabled:text-muted',
        className,
      )}
      {...props}
    />
  );
}

export interface SelectProps extends SelectHTMLAttributes<HTMLSelectElement> {}

export function Select({ className, children, ...props }: SelectProps) {
  return (
    <div className="relative inline-flex w-full items-center">
      <select
        className={cn(
          'h-9 w-full appearance-none rounded-lg border border-border bg-white pl-3 pr-8 text-sm text-foreground',
          'transition-colors duration-150 focus:outline-none focus:ring-2 focus:ring-primary focus:border-transparent',
          'disabled:bg-surface disabled:text-muted',
          className,
        )}
        {...props}
      >
        {children}
      </select>
      <ChevronDown
        size={14}
        className="pointer-events-none absolute right-2.5 text-muted"
        aria-hidden
      />
    </div>
  );
}

export function Field({
  label,
  htmlFor,
  hint,
  children,
  className,
}: {
  label: string;
  htmlFor?: string;
  hint?: string;
  children: ReactNode;
  className?: string;
}) {
  return (
    <div className={className}>
      <label htmlFor={htmlFor} className="mb-1.5 block text-sm font-medium text-foreground">
        {label}
      </label>
      {children}
      {hint && <p className="mt-1.5 text-xs leading-relaxed text-muted">{hint}</p>}
    </div>
  );
}

export function Checkbox({
  className,
  ...props
}: InputHTMLAttributes<HTMLInputElement>) {
  return (
    <input
      type="checkbox"
      className={cn(
        'h-4 w-4 shrink-0 cursor-pointer rounded border-[#C9C9C9] accent-primary transition duration-150',
        className,
      )}
      {...props}
    />
  );
}

export function Toggle({
  checked,
  onChange,
  label,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
  label: string;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      onClick={() => onChange(!checked)}
      className={cn(
        'relative h-6 w-11 shrink-0 rounded-full transition-colors duration-150',
        checked ? 'bg-primary' : 'bg-[#DDDDDD]',
      )}
    >
      <span
        className={cn(
          'absolute top-0.5 h-5 w-5 rounded-full bg-white shadow transition-all duration-150',
          checked ? 'left-[22px]' : 'left-0.5',
        )}
      />
    </button>
  );
}

/* -------------------------------- Badge ---------------------------------- */

type BadgeTone = 'neutral' | 'success' | 'error' | 'warning' | 'dark' | 'yellow';

const BADGE_TONES: Record<BadgeTone, string> = {
  neutral: 'bg-surface text-secondary border-border',
  success: 'bg-[#F0FDF4] text-success border-[#DCFCE7]',
  error: 'bg-[#FEF2F2] text-error border-[#FECACA]',
  warning: 'bg-[#FFFBEB] text-warning border-[#FDE68A]',
  dark: 'bg-soft text-white border-soft',
  yellow: 'bg-pale text-foreground border-[#F3E3A0]',
};

export function Badge({
  tone = 'neutral',
  className,
  children,
}: {
  tone?: BadgeTone;
  className?: string;
  children: ReactNode;
}) {
  return (
    <span
      className={cn(
        'inline-flex items-center gap-1 whitespace-nowrap rounded-full border px-2 py-0.5 text-[11px] font-medium',
        BADGE_TONES[tone],
        className,
      )}
    >
      {children}
    </span>
  );
}

/* -------------------------------- Tabs ----------------------------------- */

export function Segmented<T extends string>({
  value,
  onChange,
  options,
  ariaLabel,
  className,
}: {
  value: T;
  onChange: (v: T) => void;
  options: { value: T; label: string }[];
  ariaLabel: string;
  className?: string;
}) {
  return (
    <div
      role="tablist"
      aria-label={ariaLabel}
      className={cn('inline-flex rounded-lg border border-border bg-surface p-1', className)}
    >
      {options.map((o) => (
        <button
          key={o.value}
          type="button"
          role="tab"
          aria-selected={value === o.value}
          onClick={() => onChange(o.value)}
          className={cn(
            'rounded-md border px-3.5 py-1.5 text-sm font-medium transition-all duration-150',
            value === o.value
              ? 'border-border bg-white text-foreground shadow-soft'
              : 'border-transparent text-secondary hover:text-foreground',
          )}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}

/* -------------------------------- Table ---------------------------------- */

export function Table({ children, className }: { children: ReactNode; className?: string }) {
  return (
    <div className="overflow-x-auto rounded-xl border border-border bg-white shadow-soft">
      <table className={cn('w-full text-sm', className)}>{children}</table>
    </div>
  );
}

export function TH({
  children,
  className,
}: {
  children?: ReactNode;
  className?: string;
}) {
  return (
    <th
      scope="col"
      className={cn(
        'whitespace-nowrap border-b border-border bg-white px-4 py-3 text-left text-[11px] font-semibold uppercase tracking-wide text-muted',
        className,
      )}
    >
      {children}
    </th>
  );
}

export function TD({
  children,
  className,
}: {
  children?: ReactNode;
  className?: string;
}) {
  return (
    <td className={cn('border-b border-border px-4 py-3 align-middle text-secondary', className)}>
      {children}
    </td>
  );
}

export function TR({
  children,
  selected,
  className,
}: {
  children: ReactNode;
  selected?: boolean;
  className?: string;
}) {
  return (
    <tr
      className={cn(
        'transition-colors duration-150',
        selected ? 'bg-pale' : 'hover:bg-surface',
        className,
      )}
    >
      {children}
    </tr>
  );
}

/* ------------------------------ Progress --------------------------------- */

export function Progress({
  value,
  label,
}: {
  value: number | null;
  label?: string;
}) {
  const pct = Math.max(0, Math.min(100, value ?? 0));
  return (
    <div
      role="progressbar"
      aria-valuenow={Math.round(pct)}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-label={label ?? 'Progress'}
      className="h-2 w-full overflow-hidden rounded-full border border-border bg-surface"
    >
      <div
        className="h-full rounded-full bg-primary transition-all duration-150"
        style={{ width: `${pct}%` }}
      />
    </div>
  );
}

/* -------------------------------- Dialog --------------------------------- */

export function Dialog({
  open,
  onClose,
  title,
  children,
  footer,
  width = 'max-w-lg',
}: {
  open: boolean;
  onClose: () => void;
  title: string;
  children: ReactNode;
  footer?: ReactNode;
  width?: string;
}) {
  const panelRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    window.addEventListener('keydown', onKey);
    panelRef.current?.focus();
    return () => window.removeEventListener('keydown', onKey);
  }, [open, onClose]);

  if (!open) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4">
      <div
        className="absolute inset-0 bg-[rgba(17,17,17,0.4)]"
        onClick={onClose}
        aria-hidden
      />
      <div
        ref={panelRef}
        role="dialog"
        aria-modal="true"
        aria-label={title}
        tabIndex={-1}
        className={cn(
          'relative w-full rounded-xl border border-border bg-white shadow-lift focus:outline-none',
          width,
        )}
      >
        <div className="flex items-center justify-between border-b border-border px-5 py-4">
          <h2 className="text-sm font-semibold text-foreground">{title}</h2>
          <button
            type="button"
            onClick={onClose}
            aria-label="Close dialog"
            className="rounded-md p-1 text-muted transition-colors duration-150 hover:bg-surface hover:text-foreground"
          >
            <X size={16} />
          </button>
        </div>
        <div className="max-h-[70vh] overflow-y-auto px-5 py-4 text-sm text-secondary">
          {children}
        </div>
        {footer && (
          <div className="flex items-center justify-end gap-2 border-t border-border px-5 py-4">
            {footer}
          </div>
        )}
      </div>
    </div>
  );
}

/* --------------------------- States / feedback ---------------------------- */

export function Spinner({ size = 16, className }: { size?: number; className?: string }) {
  return <Loader2 size={size} className={cn('animate-spin', className)} aria-hidden />;
}

export function LoadingRow({ text = 'Loading…' }: { text?: string }) {
  return (
    <div className="flex items-center justify-center gap-2 py-10 text-sm text-muted">
      <Spinner />
      <span>{text}</span>
    </div>
  );
}

export function Skeleton({ className }: { className?: string }) {
  return <div className={cn('animate-pulse rounded-lg bg-surface', className)} aria-hidden />;
}

/** Standard page-level loading skeleton. */
export function PageLoading({ rows = 6 }: { rows?: number }) {
  return (
    <div className="space-y-4" role="status" aria-label="Loading">
      <Skeleton className="h-8 w-56" />
      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 xl:grid-cols-4">
        {Array.from({ length: 4 }).map((_, i) => (
          <Skeleton key={i} className="h-28" />
        ))}
      </div>
      <div className="space-y-2">
        {Array.from({ length: rows }).map((_, i) => (
          <Skeleton key={i} className="h-12 w-full" />
        ))}
      </div>
    </div>
  );
}

export function ErrorBanner({
  error,
  onRetry,
  className,
}: {
  error: string | null;
  onRetry?: () => void;
  className?: string;
}) {
  if (!error) return null;
  return (
    <div
      role="alert"
      className={cn(
        'flex flex-wrap items-start gap-3 rounded-lg border border-[#FECACA] bg-[#FEF2F2] px-4 py-3',
        className,
      )}
    >
      <AlertCircle size={16} className="mt-0.5 shrink-0 text-error" aria-hidden />
      <div className="min-w-0 flex-1">
        <p className="text-sm font-medium text-error">Something went wrong</p>
        <p className="mt-0.5 break-words text-sm text-[#B91C1C]">{error}</p>
      </div>
      {onRetry && (
        <Button size="sm" variant="outline" onClick={onRetry}>
          Retry
        </Button>
      )}
    </div>
  );
}

export function Notice({
  tone = 'warning',
  title,
  children,
  className,
}: {
  tone?: 'warning' | 'info' | 'success';
  title?: string;
  children: ReactNode;
  className?: string;
}) {
  const tones = {
    warning: 'border-[#FDE68A] bg-[#FFFBEB] text-warning',
    info: 'border-border bg-surface text-secondary',
    success: 'border-[#DCFCE7] bg-[#F0FDF4] text-success',
  } as const;
  return (
    <div className={cn('rounded-lg border px-4 py-3 text-sm', tones[tone], className)}>
      {title && <p className="font-semibold">{title}</p>}
      <div className={cn(title && 'mt-1')}>{children}</div>
    </div>
  );
}

export function EmptyState({
  icon: Icon = Inbox,
  title,
  description,
  action,
}: {
  icon?: LucideIcon;
  title: string;
  description: string;
  action?: ReactNode;
}) {
  return (
    <div className="flex flex-col items-center justify-center rounded-xl border border-dashed border-border bg-surface px-6 py-12 text-center">
      <div className="mb-3 flex h-11 w-11 items-center justify-center rounded-full border border-border bg-white">
        <Icon size={20} className="text-muted" aria-hidden />
      </div>
      <h3 className="text-sm font-semibold text-foreground">{title}</h3>
      <p className="mt-1 max-w-md text-sm leading-relaxed text-muted">{description}</p>
      {action && <div className="mt-4">{action}</div>}
    </div>
  );
}

/* ------------------------------- StatCard -------------------------------- */

export function StatCard({
  label,
  value,
  hint,
  icon: Icon,
}: {
  label: string;
  value: string | number;
  hint?: string;
  icon?: LucideIcon;
}) {
  return (
    <Card className="p-5">
      <div className="flex items-start justify-between gap-2">
        <p className="text-sm font-medium text-secondary">{label}</p>
        {Icon && <Icon size={16} className="mt-0.5 shrink-0 text-muted" aria-hidden />}
      </div>
      <p className="mt-3 text-2xl font-semibold leading-none text-foreground">{value}</p>
      {/* Key stat accent: yellow bar (yellow text on white fails contrast). */}
      <div className="mt-2.5 h-1 w-10 rounded-full bg-primary" aria-hidden />
      {hint && <p className="mt-2 text-xs text-muted">{hint}</p>}
    </Card>
  );
}
