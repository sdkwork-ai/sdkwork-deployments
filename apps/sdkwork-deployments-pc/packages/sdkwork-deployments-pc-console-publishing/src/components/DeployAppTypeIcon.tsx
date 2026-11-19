/**
 * Inline SVG glyphs for the application-type grid (icon + name cards).
 * All glyphs are geometric, `currentColor`-filled and self-contained, so they
 * inherit the dialog theme without external assets.
 */
import type { DeployAppTypeIconId } from "../service/deploy-app-publishing.ts";

export interface DeployAppTypeIconProps {
  readonly iconKey: DeployAppTypeIconId
  readonly size?: number
  readonly className?: string
}

/** @returns the glyph for one type card. */
export function DeployAppTypeIcon({ iconKey, size = 26, className }: DeployAppTypeIconProps) {
  const common = {
    width: size,
    height: size,
    viewBox: "0 0 24 24",
    fill: "none",
    xmlns: "http://www.w3.org/2000/svg",
    "aria-hidden": true as const,
    className,
  };
  switch (iconKey) {
    case "h5":
      return (
        <svg {...common}>
          <rect x="6.5" y="2.5" width="11" height="19" rx="2.5" stroke="currentColor" strokeWidth="1.7" />
          <path d="M9.5 9.5 8 12l1.5 2.5M14.5 9.5 16 12l-1.5 2.5M12.8 9l-1.6 6" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" />
        </svg>
      );
    case "pc":
      return (
        <svg {...common}>
          <rect x="2.5" y="4" width="19" height="12.5" rx="2" stroke="currentColor" strokeWidth="1.7" />
          <path d="M8.5 20.5h7M12 16.5v4" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" />
        </svg>
      );
    case "desktop":
      return (
        <svg {...common}>
          <rect x="2.5" y="3.5" width="19" height="17" rx="2.5" stroke="currentColor" strokeWidth="1.7" />
          <path d="M2.5 8h19" stroke="currentColor" strokeWidth="1.7" />
          <circle cx="5.6" cy="5.8" r="0.9" fill="currentColor" />
          <circle cx="8.4" cy="5.8" r="0.9" fill="currentColor" />
          <rect x="5.5" y="11" width="6" height="6" rx="1.2" stroke="currentColor" strokeWidth="1.6" />
          <path d="M14.5 12.5h4M14.5 15.5h4" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" />
        </svg>
      );
    case "mini-program":
      return (
        <svg {...common}>
          <rect x="3" y="3" width="8" height="8" rx="2" stroke="currentColor" strokeWidth="1.7" />
          <rect x="13" y="3" width="8" height="8" rx="2" stroke="currentColor" strokeWidth="1.7" />
          <rect x="3" y="13" width="8" height="8" rx="2" stroke="currentColor" strokeWidth="1.7" />
          <path d="M17 13.5v7M13.5 17h7" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" />
        </svg>
      );
    case "android":
      return (
        <svg {...common}>
          <path d="M7 10a5 5 0 0 1 10 0v5.5a1.5 1.5 0 0 1-1.5 1.5h-7A1.5 1.5 0 0 1 7 15.5V10Z" stroke="currentColor" strokeWidth="1.7" />
          <path d="M8.6 6.6 7.4 4.9M15.4 6.6l1.2-1.7M5 12H3.5M20.5 12H19M9.5 14.2v1.6M14.5 14.2v1.6" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" />
        </svg>
      );
    case "ios":
      return (
        <svg {...common}>
          <rect x="6.5" y="2.5" width="11" height="19" rx="2.8" stroke="currentColor" strokeWidth="1.7" />
          <path d="M10.5 5h3" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" />
          <circle cx="12" cy="18" r="1.1" fill="currentColor" />
        </svg>
      );
    case "harmony":
      return (
        <svg {...common}>
          <circle cx="12" cy="12" r="8.5" stroke="currentColor" strokeWidth="1.7" />
          <path d="M8 9.2c2.6-1.7 5.4-1.7 8 0M8 14.8c2.6 1.7 5.4 1.7 8 0" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" />
        </svg>
      );
    case "flutter":
      return (
        <svg {...common}>
          <path d="M4.5 12.5 12 5h6.5l-6 6 6 6H12l-7.5-7.5Z" stroke="currentColor" strokeWidth="1.7" strokeLinejoin="round" />
          <path d="M10 15.5 12.5 18H19l-3.5-3.5" stroke="currentColor" strokeWidth="1.7" strokeLinejoin="round" />
        </svg>
      );
    case "api":
      return (
        <svg {...common}>
          <path d="M8.5 4.5 4 12l4.5 7.5M15.5 4.5 20 12l-4.5 7.5" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" />
          <circle cx="12" cy="12" r="1.4" fill="currentColor" />
        </svg>
      );
    case "static":
      return (
        <svg {...common}>
          <path d="M6 3.5h8L19 8.5v12a1 1 0 0 1-1 1H6a1 1 0 0 1-1-1v-16a1 1 0 0 1 1-1Z" stroke="currentColor" strokeWidth="1.7" strokeLinejoin="round" />
          <path d="M13.5 3.8V9h5" stroke="currentColor" strokeWidth="1.6" strokeLinejoin="round" />
        </svg>
      );
  }
}
