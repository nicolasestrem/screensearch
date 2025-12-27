/**
 * Framer Motion animation variants for consistent animations throughout the app.
 * Based on the AI-First UI redesign spec.
 */

// Modal animations
export const modalVariants = {
  hidden: { opacity: 0, scale: 0.95, y: -20 },
  visible: {
    opacity: 1,
    scale: 1,
    y: 0,
    transition: { type: 'spring', damping: 25, stiffness: 300 },
  },
  exit: { opacity: 0, scale: 0.95, y: -20 },
};

export const backdropVariants = {
  hidden: { opacity: 0 },
  visible: { opacity: 1 },
  exit: { opacity: 0 },
};

// Stagger container for lists
export const staggerContainer = {
  hidden: { opacity: 0 },
  visible: {
    opacity: 1,
    transition: { staggerChildren: 0.1 },
  },
};

// Individual item animations
export const fadeInUp = {
  hidden: { opacity: 0, y: 20 },
  visible: {
    opacity: 1,
    y: 0,
    transition: { duration: 0.3, ease: 'easeOut' },
  },
};

export const fadeInLeft = {
  hidden: { opacity: 0, x: -20 },
  visible: {
    opacity: 1,
    x: 0,
    transition: { duration: 0.3, ease: 'easeOut' },
  },
};

export const fadeInRight = {
  hidden: { opacity: 0, x: 20 },
  visible: {
    opacity: 1,
    x: 0,
    transition: { duration: 0.3, ease: 'easeOut' },
  },
};

export const scaleIn = {
  hidden: { opacity: 0, scale: 0.95 },
  visible: {
    opacity: 1,
    scale: 1,
    transition: { duration: 0.2, ease: 'easeOut' },
  },
};

// Sidebar collapse animations
export const sidebarVariants = {
  expanded: { width: 256 },
  collapsed: { width: 72 },
};

export const sidebarContentVariants = {
  hidden: { opacity: 0, width: 0 },
  visible: {
    opacity: 1,
    width: 'auto',
    transition: { duration: 0.2 },
  },
  exit: {
    opacity: 0,
    width: 0,
    transition: { duration: 0.2 },
  },
};

// Page transitions
export const pageVariants = {
  initial: { opacity: 0, y: 10 },
  animate: {
    opacity: 1,
    y: 0,
    transition: { duration: 0.4, ease: 'easeOut' },
  },
  exit: {
    opacity: 0,
    y: -10,
    transition: { duration: 0.2 },
  },
};

// Card hover animation (for interactive elements)
export const cardHoverVariants = {
  rest: {
    scale: 1,
    boxShadow: '0 0 0 0 rgba(0, 212, 255, 0)',
  },
  hover: {
    scale: 1.02,
    boxShadow: '0 0 20px -5px rgba(0, 212, 255, 0.3)',
    transition: { duration: 0.2, ease: 'easeInOut' },
  },
  tap: { scale: 0.98 },
};

// Glow pulse for active states
export const glowPulseVariants = {
  initial: { boxShadow: '0 0 10px 0 rgba(0, 212, 255, 0.3)' },
  animate: {
    boxShadow: [
      '0 0 10px 0 rgba(0, 212, 255, 0.3)',
      '0 0 20px 5px rgba(0, 212, 255, 0.5)',
      '0 0 10px 0 rgba(0, 212, 255, 0.3)',
    ],
    transition: { duration: 2, repeat: Infinity, ease: 'easeInOut' },
  },
};

// Spring configs for common use cases
export const springConfig = {
  snappy: { type: 'spring', damping: 25, stiffness: 300 },
  bouncy: { type: 'spring', damping: 15, stiffness: 200 },
  smooth: { type: 'spring', damping: 30, stiffness: 150 },
};
