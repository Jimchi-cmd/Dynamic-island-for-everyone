# The download website

https://jimchi-cmd.github.io/Dynamic-island-for-everyone/ is built from `site/`, on GitHub Pages:

- `site/index.html`, `style.css`: the page. Its `{{…}}` slots (version, date, each download's link and size) are filled from the latest release when it is built.
- `site/download.js`: points the big button at the visitor's device (Windows, Mac, Linux or Android; iPhone gets a note instead), from the browser's own user agent. Nothing is sent anywhere.
- `site/reels.js`: the four clips play only when clicked. `site/spotlight.js`: a soft light on the top picture that follows the pointer.
- `site/media/`: pictures and clips, made by `scripts/site/make-media.py` from the island's own frames and the promo reels (see [promo.md](promo.md) to make those first).
- `site/fonts/`: Geist (SIL Open Font License), served with the site: the page loads nothing from other sites, sets no cookies and has no analytics.

Build it locally:

```bash
gh api repos/Jimchi-cmd/Dynamic-island-for-everyone/releases/latest > release.json
```
```bash
node scripts/site/build.mjs release.json build/site
```
```bash
cd build/site && python3 -m http.server 8000
```

The **Website** workflow (`.github/workflows/pages.yml`) builds and publishes it when `site/` changes on `main`, after every release (the Publish workflow calls it), or by hand from the Actions tab. GitHub Pages must be set to **Settings → Pages → Source: GitHub Actions** once.

The design follows a layout checker's rules (one section per screen, equal edges, a hero entrance, hover movement, text contrast); `npm test` checks that every slot fills from a release and that the page links nowhere but GitHub.
