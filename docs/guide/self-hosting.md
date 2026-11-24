# Ipse on a home server

Run Ipse on your NAS, a Raspberry Pi or any small server at home, without a desktop. The memory lives there, the calendars, contacts and other sources you follow sync in the background, and you reach it from your other machines: the interface in a browser, your AI apps over MCP.

One process does all three: `ipse serve`. The Docker image `ghcr.io/loren844/ipse` runs it for `linux/amd64` and `linux/arm64` (Synology, Raspberry Pi 4 and 5) as a user who is not root. Decision [0019](../decisions/0019-reached-from-the-network.md) says why it works this way.

## What changes when the memory can be reached from your network

On your computer, only you reach Ipse. On a server, anyone on your network can reach the port. Ipse then asks for a code every time:

- **The interface** asks for its own access code, made with `ipse access-code`. Keep it like a password: it opens your whole memory, as you.
- **Each AI app** sends its own access code, as on your computer, and sees only the categories you opened to it.
- **HTTPS is your reverse proxy's job** (Caddy, Traefik, the one of your NAS). Without it, the codes and what Ipse answers cross your network in plain text, readable by any device on it. Keep plain HTTP for a home network you trust, and never open the port to the internet without HTTPS in front.

## 1. Get ready

You need Docker with Compose. In a folder of your server, take [`docker-compose.yml`](../../docker-compose.yml) from this repository, and make the two folders it uses yourself, so that they belong to you and not to `root`:

```sh
mkdir ipse && cd ipse
curl -fsSLO https://raw.githubusercontent.com/Loren844/ipse/main/docker-compose.yml
mkdir vault secrets
```

- `vault/` holds your memory: plain Markdown files.
- `secrets/` holds the secrets of your sources (calendar links, app passwords, tokens), in `secrets.json`, readable only by you. A container has no keychain, so they go there, apart from your memory: a copy of `vault/` carries none of them.

Ipse runs as your user (`1000:1000` by default). If `id -u` and `id -g` say otherwise, write them in a `.env` file next to `docker-compose.yml`:

```sh
printf 'UID=%s\nGID=%s\n' "$(id -u)" "$(id -g)" > .env
```

## 2. Create the memory, or bring yours

A new memory, in French, in your time zone (a container's clock is in UTC, so give yours):

```sh
docker compose run --rm ipse init --name "Your Name" --language fr --time-zone Europe/Paris
```

Or copy the folder of an existing memory into `vault/` (its files, not the folder itself), with Ipse stopped on the computer it comes from. Sources followed there need their secrets again: connect them once more from **Sources**.

## 3. Start it

```sh
docker compose up -d
docker compose logs ipse
```

The log says where Ipse serves and that it can be reached from the network. Then make the interface's access code:

```sh
docker compose exec ipse ipse access-code
```

It is shown once. Open `http://<your server>:7420/`, enter it, and you are in your memory. The session lasts 30 days, or until Ipse restarts. Running `ipse access-code` again replaces the code and closes every session opened with the old one: do it if the code leaked.

## 4. Connect an AI app from another machine

In the interface, **IA connectées** (**Connected AIs**) adds an AI and shows its code, or:

```sh
docker compose exec ipse ipse client add "Claude"
```

An AI app that connects by address takes:

- the address `https://ipse.example.org/mcp` (behind your proxy) or `http://<your server>:7420/mcp`;
- the header `Authorization: Bearer ipse_…`, with that AI's code.

Apps that only start a program (`command` and `args`) cannot reach a server this way; [Connecting AI apps](connecting.md) covers them on your computer.

## 5. HTTPS with a reverse proxy

Ipse serves plain HTTP, and the proxy in front of it serves HTTPS. Caddy and Traefik send `X-Forwarded-Proto: https`, which tells Ipse to mark the session cookie `Secure`. When the proxy runs on the same server, keep Ipse's port to that server only: in `docker-compose.yml`, write `"127.0.0.1:7420:7420"`.

**Caddy**, with a certificate it gets on its own for a name that points to your server (`Caddyfile`):

```
ipse.example.org {
	reverse_proxy 127.0.0.1:7420
}
```

On a home network without a public name, `tls internal` in that block makes Caddy use its own certificate authority; install its root certificate on your devices.

**Traefik**, in the same Compose file: drop `ports:` from the `ipse` service and add, with your own entry point and resolver names:

```yaml
    labels:
      - traefik.enable=true
      - traefik.http.routers.ipse.rule=Host(`ipse.example.org`)
      - traefik.http.routers.ipse.entrypoints=websecure
      - traefik.http.routers.ipse.tls.certresolver=letsencrypt
      - traefik.http.services.ipse.loadbalancer.server.port=7420
```

A login of your proxy (basic auth, an SSO) in front of the interface is fine, but leave `/mcp` without it: AI apps send only their `Authorization` header.

## Backups

`vault/` is plain files: back it up as any folder, with Ipse running or not (its writes are atomic). `secrets/` is not needed to restore your memory: without it, reconnect your sources. If you back it up, keep that copy as safe as the passwords it holds.

## Updates

```sh
docker compose pull && docker compose up -d
```

`latest` follows each release. To choose when you update, write a version instead, like `ghcr.io/loren844/ipse:0.5.0`, and change it when the [changelog](../../CHANGELOG.md) says a new one is out: it also says when a version changes the format of the memory's files.

## Without Docker

The same on any Linux server, with the `ipse` command from the [releases](https://github.com/Loren844/ipse/releases/latest):

```sh
export IPSE_SECRETS_FILE=/home/you/.config/ipse/secrets.json
ipse access-code --vault /home/you/Ipse
ipse serve --vault /home/you/Ipse --listen 0.0.0.0:7420
```

Without `--listen`, `ipse serve` listens on this computer only (`127.0.0.1:7420`). Run it as a service of your system (a systemd unit with `Restart=on-failure`) so that it starts with the server.
