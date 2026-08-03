interface IconProps {
  size?: number;
}

const base = (size: number) => ({
  width: size,
  height: size,
  viewBox: "0 0 16 16",
  fill: "none",
  stroke: "currentColor",
  strokeWidth: 1.5,
  strokeLinecap: "round" as const,
  strokeLinejoin: "round" as const,
  "aria-hidden": true,
});

export const RefreshIcon = ({ size = 14 }: IconProps) => (
  <svg {...base(size)}>
    <path d="M14 8a6 6 0 1 1-1.76-4.24" />
    <path d="M14 2v4h-4" />
  </svg>
);

export const SettingsIcon = ({ size = 14 }: IconProps) => (
  <svg {...base(size)}>
    <circle cx="8" cy="8" r="2.25" />
    <path d="M8 1.5v1.6M8 12.9v1.6M14.5 8h-1.6M3.1 8H1.5M12.6 3.4l-1.1 1.1M4.5 11.5l-1.1 1.1M12.6 12.6l-1.1-1.1M4.5 4.5 3.4 3.4" />
  </svg>
);

export const ChevronIcon = ({ size = 14 }: IconProps) => (
  <svg {...base(size)}>
    <path d="m4 6 4 4 4-4" />
  </svg>
);

export const CloseIcon = ({ size = 14 }: IconProps) => (
  <svg {...base(size)}>
    <path d="m4 4 8 8M12 4l-8 8" />
  </svg>
);

export const AlertIcon = ({ size = 13 }: IconProps) => (
  <svg {...base(size)}>
    <circle cx="8" cy="8" r="6.25" />
    <path d="M8 5v3.5M8 11h.01" />
  </svg>
);

export const KeyIcon = ({ size = 13 }: IconProps) => (
  <svg {...base(size)}>
    <circle cx="5.5" cy="8" r="2.5" />
    <path d="M8 8h6.5M12.5 8v2M10.5 8v1.5" />
  </svg>
);
