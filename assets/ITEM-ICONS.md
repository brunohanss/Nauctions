# Demo item icons

The 50 demo items use real World of Warcraft Classic item IDs and names. Auction prices and observations in the demo are synthetic.

Run `./scripts/download-demo-icons.ps1` from PowerShell to check cached metadata and images and rebuild `assets/item-catalog.json`. Use `-Refresh` to download each file again. This operation needs Internet access.

The script gets each item name and icon name from the public Wowhead Classic tooltip endpoint. It requires an exact match with `fixtures/demo/items.json`. The original metadata responses are stored in `assets/item-icon-metadata/`. The script downloads the corresponding 56 by 56 JPEG from Blizzard's public artwork CDN. It decodes each image and checks its dimensions and format.

The catalogue records the item page, metadata endpoint, image URL, icon name, and SHA-256 image hash for each item. Images are stored in `web/public/item-icons/` and are served locally. The app does not need a remote image request.

Item metadata: Wowhead. World of Warcraft game artwork: Blizzard Entertainment. These files do not imply affiliation or endorsement.
