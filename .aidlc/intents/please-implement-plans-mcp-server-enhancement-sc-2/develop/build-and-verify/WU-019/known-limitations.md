# Known limitations: WU-019

- Not checked in a browser (manual, TS-030 checklist): dark mode, forced-colors, keyboard flow and focus survival across auto-refresh, screen-reader announcements, widths between 360 and 960 px other than those listed.
- Near the 60rem breakpoint (960-1040 px with the detail panel open) the event column is narrow (47-127 px), so long event names wrap to several lines; it fits, but looks cramped. Option for later: raise the breakpoint to about 66rem.
- The review-fix static assertions match exact spellings (for example the banner container class); they guard the fixes as written, not every equivalent regression.
- Windows browsers untested.
