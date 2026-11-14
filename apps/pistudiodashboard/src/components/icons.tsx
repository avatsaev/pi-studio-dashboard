// The few glyphs the app needs, drawn here as inline SVG (no icon dependency; design-system § Icons).
// Decorative: the control that holds one carries its own accessible name.

interface IconProps {
  /** Pixel size of the square; defaults to the 14 px icon size. */
  size?: number;
}

function Svg({ size = 14, children }: IconProps & { children: React.ReactNode }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.5"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      focusable="false"
    >
      {children}
    </svg>
  );
}

export function CloseIcon(props: IconProps) {
  return (
    <Svg {...props}>
      <path d="M4 4l8 8M12 4l-8 8" />
    </Svg>
  );
}

export function AlertIcon(props: IconProps) {
  return (
    <Svg {...props}>
      <path d="M8 2.2l6.2 11H1.8z" />
      <path d="M8 6.5v3.2M8 11.6v.1" />
    </Svg>
  );
}

/** The glyph of a tool call kind (`shell`, `read`, `write`/`edit`, `search`, `fetch`; anything else is a task). */
export function ToolKindIcon({ kind, ...props }: IconProps & { kind: string }) {
  switch (kind) {
    case "shell":
      return (
        <Svg {...props}>
          <path d="M2.5 4.5l4 3.5-4 3.5M8 12h5.5" />
        </Svg>
      );
    case "read":
      return (
        <Svg {...props}>
          <path d="M4 1.8h5.2L12.5 5v9.2H4zM9 1.8V5.2h3.5M6 8.5h4M6 11h4" />
        </Svg>
      );
    case "write":
    case "edit":
      return (
        <Svg {...props}>
          <path d="M2.5 13.5l.6-3L10.8 2.8a1.4 1.4 0 012 2L5 12.6zM9.6 4l2.4 2.4" />
        </Svg>
      );
    case "search":
      return (
        <Svg {...props}>
          <path d="M7 11.8a4.8 4.8 0 100-9.6 4.8 4.8 0 000 9.6zM10.5 10.5l3.3 3.3" />
        </Svg>
      );
    case "fetch":
      return (
        <Svg {...props}>
          <path d="M8 14.2a6.2 6.2 0 100-12.4 6.2 6.2 0 000 12.4zM1.9 8h12.2M8 1.8c1.8 1.7 2.7 3.8 2.7 6.2S9.8 12.5 8 14.2M8 1.8C6.2 3.5 5.3 5.6 5.3 8s.9 4.5 2.7 6.2" />
        </Svg>
      );
    default:
      return (
        <Svg {...props}>
          <path d="M2.5 4h11M2.5 8h11M2.5 12h7" />
        </Svg>
      );
  }
}
