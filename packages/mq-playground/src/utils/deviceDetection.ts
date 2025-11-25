/**
 * Mobile device detection utility
 */

/**
 * Breakpoint for mobile devices (in pixels)
 */
export const MOBILE_BREAKPOINT = 768;

/**
 * Check if the current viewport is mobile size
 * @returns true if viewport width is less than or equal to mobile breakpoint
 */
export const isMobile = (): boolean => {
  return window.innerWidth <= MOBILE_BREAKPOINT;
};

/**
 * Check if the current viewport is desktop size
 * @returns true if viewport width is greater than mobile breakpoint
 */
export const isDesktop = (): boolean => {
  return window.innerWidth > MOBILE_BREAKPOINT;
};
