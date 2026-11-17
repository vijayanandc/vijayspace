// Buttons: primary / secondary / danger / ghost (subtle), optional leading icon, and a router link styled as a button.

import type { LucideIcon } from 'lucide-react';
import type { ButtonHTMLAttributes, ReactNode } from 'react';
import { Link } from 'react-router-dom';

type Variant = 'primary' | 'secondary' | 'danger' | 'ghost';

interface Props extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: Variant;
  icon?: LucideIcon;
  size?: 'md' | 'sm';
}

function classes(variant: Variant, size: 'md' | 'sm', className?: string): string {
  return `btn btn-${variant}${size === 'sm' ? ' btn-sm' : ''}${className ? ` ${className}` : ''}`;
}

export function Button({ variant = 'secondary', size = 'md', icon: Icon, className, children, ...rest }: Props) {
  return (
    <button type="button" className={classes(variant, size, className)} {...rest}>
      {Icon && <Icon size={16} aria-hidden="true" />}
      {children}
    </button>
  );
}

interface LinkProps {
  to: string;
  variant?: Variant;
  icon?: LucideIcon;
  size?: 'md' | 'sm';
  children: ReactNode;
  title?: string;
}

export function LinkButton({ to, variant = 'secondary', size = 'md', icon: Icon, children, title }: LinkProps) {
  return (
    <Link to={to} className={classes(variant, size)} title={title}>
      {Icon && <Icon size={16} aria-hidden="true" />}
      {children}
    </Link>
  );
}
