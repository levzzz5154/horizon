# Pack-finder visual evidence for arismoko/horizon#1297

Actual game captures at commit `5487072f58676ca3bc39fdb2ab165b00a2259369`, using a packaged WebAssembly test mod in fresh scratch game/config/data homes. These captures predate the latest main integration.

1. `packs-before.png`: optional contributed pack is available and off.
2. `packs-selected.png`: selected through the pack screen.
3. `packs-applied-header.png`: after Done/reload, the test pack changes the header to “Optional pack applied”. The fixture archive was atomically replaced to add this visible language override before re-selection.
4. `packs-disabled.png`: after disabling and reloading, the header is restored.

The pack-list translated title remained cached during this run; the page header demonstrates the applied resource generation. The game exited cleanly. These are rendered screenshots; no raw game assets or prepared resources are included.
