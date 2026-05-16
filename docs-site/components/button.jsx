import Link from "next/link";

const SIZES = {
  sm: "h-8 px-4 text-[0.8125rem]",
  md: "h-10 px-6 text-sm",
  lg: "h-11 gap-2 px-8 text-[0.9375rem]",
};

const VARIANTS = {
  default: "border-border bg-bg text-fg hover:border-fg",
  primary:
    "border-accent bg-accent text-accent-fg hover:bg-transparent hover:text-accent",
};

function classes({ variant = "default", size = "md", className = "" }) {
  return [
    "inline-flex items-center justify-center rounded-md border font-medium no-underline transition",
    VARIANTS[variant],
    SIZES[size],
    className,
  ]
    .filter(Boolean)
    .join(" ");
}

export function Button({ variant, size, className, ...rest }) {
  return <button className={classes({ variant, size, className })} {...rest} />;
}

export function ButtonLink({ variant, size, className, ...rest }) {
  return <Link className={classes({ variant, size, className })} {...rest} />;
}
