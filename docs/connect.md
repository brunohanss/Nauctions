# Connect to Nauctions

Server: **https://nauctions.when-to-plant.net**. Market: **Normal / Horde**.

1. Download both packages from [Nauctions 0.4.1](https://github.com/brunohanss/Nauctions/releases/tag/v0.4.1): `Nauctions_0.4.1_x64-setup.exe` and `nauctions-ahdb.zip`.
2. Close WoW. Back up the existing `AuctionDB` addon and its account SavedVariables. Extract the addon ZIP into `Interface/AddOns`. Check that the path ends in `AuctionDB/AuctionDB.toc` and that only one AHDB copy is enabled.
3. Run the companion installer. Enter the server address above without an added path.
4. Enter your nickname and invitation code **COPAINS**. Use the same nickname to reconnect to an existing friend account. Nicknames are not case-sensitive. Anyone with this code can access a friend account by its nickname. Administrator and disabled accounts cannot use this method. Keep the remembered membership on this computer.
5. Select the WoW folder that contains `WTF/Account`. Pair collection. Select Normal / Horde.
6. Enable **Record full scans**, run an AH scan, then reload or log out. Confirm that the companion reports an accepted upload.
7. Check tooltips and professions for errors. Quit Nauctions from its tray and reopen it. Confirm that settings remain available.

Closing the companion window keeps it in the tray. **Quit Nauctions** stops it. Startup and automatic collection can be disabled in Settings.

The companion updater updates the app and collector. Addon updates use a separate ZIP and require WoW to be closed. Do not reinstall WoW to repair an addon problem: disable AuctionDB and retain its saved files and diagnostics first.

Real-game compatibility still needs the first-client trial. Shared-history writes remain disabled. The installer has no Windows publisher certificate; this is separate from its verified updater signature. Check each package against its `.sha256` file.

Public HTTPS, native sign-in, pairing and one synthetic upload passed on 30 October. Public health, capabilities and the unauthenticated API boundary passed again on 31 October. These checks do not replace a real-game scan.
