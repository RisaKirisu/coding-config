# 05: One UI and one lifecycle API

**What to build:** The Project app provides every function of the management page, then the management page and the blocking lifecycle API are removed, leaving daemon-owned operations as the only way to start, stop, delete, launch, and restart.

**Blocked by:** 03

**Status:** ready-for-agent

- [ ] The Project app registers a Project through the Project Browser and unregisters one.
- [ ] Per Project, it shows memory use, Project ID, Project host, and separate local and remote DSH links, and opens a guest port with its Project URL.
- [ ] It shows and runs Session Sync setup and deletes a Project's Sync Store data.
- [ ] The Project Log viewer has source filters, errors only, and Follow.
- [ ] The management page, its dialog, the blocking lifecycle routes, and request-bound operations with disconnect cancellation are removed; the management route returns 404.
- [ ] Operation tests: a conflicting submission is rejected; stop and delete cancel an active start and wait for process cleanup; two clients see the same active and latest operation.
- [ ] The existing browser checks cover the new panels on desktop and mobile layouts.
- [ ] The readme, the Project app docs, and the lifecycle paragraph in `AGENTS.md` describe the single interface.
