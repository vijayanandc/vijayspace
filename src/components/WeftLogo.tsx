// The Weft mark: two interlaced threads on a rounded tile (own artwork).

export function WeftLogo({ size = 24 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" aria-hidden="true" className="weft-logo">
      <rect width="24" height="24" rx="6" fill="#ffffff" />
      <path d="M4.5 9c2.5 0 3.2 6 5.6 6s3.2-6 5.6-6 2.4 3 3.8 3" fill="none" stroke="#0f6cbd" strokeWidth="2.2" strokeLinecap="round" />
      <path d="M4.5 15c2.5 0 3.2-6 5.6-6s3.2 6 5.6 6 2.4-3 3.8-3" fill="none" stroke="#62abf5" strokeWidth="2.2" strokeLinecap="round" />
    </svg>
  );
}
