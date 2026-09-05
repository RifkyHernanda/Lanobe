import SvgIcon, { SvgIconProps } from '@mui/material/SvgIcon';

/**
 * Lanobe's mark: an open book on a dark disc.
 *
 * Deliberately plain and geometric so it stays legible at favicon sizes, where
 * the 96x96 PNG is rendered down to 16px. Kept in sync by hand with
 * `WebUI/public/favicon.svg` -- if you change one, change the other, or the tab
 * icon and the splash screen drift apart.
 */
export const LanobeLogo = ({ circleRingColor = '#d9a441', ...props }: SvgIconProps & { circleRingColor?: string }) => (
    <SvgIcon {...props}>
        <svg version="1.1" viewBox="0 0 100 100" xmlns="http://www.w3.org/2000/svg">
            <circle cx="50" cy="50" r="50" fill={circleRingColor} />
            <circle cx="50" cy="50" r="39" fill="#1a1a1d" />
            <path d="M27 35.5q10.5-3 21 2.5v28q-10.5-5.5-21-2.5z" fill="#fdfdfd" />
            <path d="M73 35.5q-10.5-3-21 2.5v28q10.5-5.5 21-2.5z" fill="#e9e9ec" />
            <rect x="49" y="37" width="2" height="29" rx="1" fill={circleRingColor} />
        </svg>
    </SvgIcon>
);
