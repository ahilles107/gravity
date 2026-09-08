# Gravity marketing site

Static Vite site with a small Cloudflare Worker route that resolves the latest
macOS release from Gravity's updater manifest.

```sh
pnpm install
pnpm dev
pnpm check
```

## Cloudflare

`wrangler.jsonc` serves the built files through Workers Static Assets on your
account's `workers.dev` domain. It does not require the Gravity domain or account.

```sh
pnpm worker:dev
pnpm deploy
```

Deployment requires your own Cloudflare account and credentials accepted by
Wrangler. Set `vars.RELEASE_MANIFEST_URL` to your HTTPS updater manifest to enable
`/download/latest`; it returns 503 without configuration. Add a `routes` entry
with your custom domain if desired. Review the canonical URLs, sitemap, and
branding when forking.
