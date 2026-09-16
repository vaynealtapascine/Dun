/** Which shell the UI is running in. */
export const isPreview = "__DUN_PREVIEW__" in window;

/** The Android app (not a phone-sized browser preview). */
export const isPhone = navigator.userAgent.includes("Android") && !isPreview;

/** The Windows app: everything the phone doesn't have (tray, hotkey, backups). */
export const isDesktop = !isPhone;
