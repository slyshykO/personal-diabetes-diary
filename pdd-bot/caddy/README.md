# Caddy Setup

This folder contains a simple production shape for exposing the bot web UI on the public internet:

`Internet -> Caddy (HTTPS + basic auth) -> pdd-bot on 127.0.0.1:8080`

## Files

- `Caddyfile`: reverse proxy for the SPA and `/api`
- `.env.example`: variables used by the `Caddyfile`

## Recommended app setting

Keep the bot HTTP server bound only to localhost in [config.toml](../config.toml):

```toml
[html_config]
enable = true
listen = "127.0.0.1:8080"
allow = []
```

Do not expose port `8080` in Oracle firewall or security list rules. Only ports `80` and `443` should be public.

## Install the Caddy config

1. Copy [Caddyfile](./Caddyfile) to `/etc/caddy/Caddyfile`.
2. Generate a password hash:

```bash
caddy hash-password --plaintext 'change-me-now'
```

3. Create `/etc/caddy/.env` from [`.env.example`](./.env.example) and replace all example values.

Example:

```text
CADDY_ACME_EMAIL=admin@example.com
PDD_DOMAIN=diary.example.com
PDD_BASIC_AUTH_USER=alex
PDD_BASIC_AUTH_HASH=$2a$14$...
```

## Make systemd load the env file

Create a Caddy service override:

```bash
sudo systemctl edit caddy
```

Put this into the editor:

```ini
[Service]
EnvironmentFile=/etc/caddy/.env
```

Then reload systemd:

```bash
sudo systemctl daemon-reload
```

## Validate and reload

Validate the config before reloading:

```bash
sudo caddy validate --config /etc/caddy/Caddyfile --adapter caddyfile
```

If validation passes:

```bash
sudo systemctl reload caddy
```

Useful checks:

```bash
systemctl status caddy
journalctl -u caddy --no-pager | less +G
```

## Notes

- This protects the whole site, including the SPA and `/api`, before requests reach the Rust app.
- The browser will show the standard HTTP basic auth login prompt. There is no custom login page in the app.
- If you later want a nicer login screen or logout flow, add app-side cookie sessions later and keep Caddy as the TLS reverse proxy.
