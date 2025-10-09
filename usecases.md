# Usecases

Some common uses for Docker Proxy Filter with popular applications.

## Homepage Docker Integration

[Homepage](https://gethomepage.dev/), a popular startpage application, can [use the Docker API](https://gethomepage.dev/configs/docker/) to discover services automatically for its dashboard.

Homepage uses only the `/containers/json` endpoint to find services by label and parse running state. There is no need for it to have access to other non-homepage labeled services.

Use the [`CONTAINER_LABELS`](/README.md#container_labels) environmental to allow any label with a key containing `homepage`:

```yaml
services:
  proxy-container:
    image: foxxmd/docker-proxy-filter
    environment:
      - PROXY_URL=http://socket-proxy:2375
      - CONTAINER_LABELS=homepage
    ports:
      - 2375:2375
  socket-proxy:
    image: wollomatic/socket-proxy:1.10.0
    restart: unless-stopped
    user: 0:0
    mem_limit: 64M
    read_only: true
    cap_drop:
      - ALL
    security_opt:
      - no-new-privileges
    command:
      - '-loglevel=debug'
      - '-listenip=0.0.0.0'
      - '-allowfrom=proxy-container'
      - '-allowHEAD=/_ping'
      - '-allowGET=/_ping|/(v1\..{1,2}/)?(info|version|containers|events).*'
    volumes:
      - /var/run/docker.sock:/var/run/docker.sock:ro
```

## Uptime Kuma Docker Monitor

[Uptime Kuma](https://github.com/louislam/uptime-kuma) can create [monitors for Docker containers](https://github.com/louislam/uptime-kuma/wiki/How-to-Monitor-Docker-Containers) by using the Docker API through either direct socket connection or TCP/HTTP.

In Uptime Kuma, setup a new Docker Host using docker-proxy-filter instead of a normal socket-proxy. Then, add a label (`uptime.enabled=true`) on for each service you want Uptime Kuma to be able to monitor. Finally, add that label to `CONTAINER_LABELS` for docker-proxy-filter.

```yaml
services:
  proxy-container:
    image: foxxmd/docker-proxy-filter
    environment:
      - PROXY_URL=http://socket-proxy:2375
      - CONTAINER_LABELS=uptime.enabled=true
    ports:
      - 2375:2375
  socket-proxy:
    image: wollomatic/socket-proxy:1.10.0
    restart: unless-stopped
    user: 0:0
    mem_limit: 64M
    read_only: true
    cap_drop:
      - ALL
    security_opt:
      - no-new-privileges
    command:
      - '-loglevel=debug'
      - '-listenip=0.0.0.0'
      - '-allowfrom=proxy-container'
      - '-allowHEAD=/_ping'
      - '-allowGET=/_ping|/(v1\..{1,2}/)?(info|version|containers|events).*'
    volumes:
      - /var/run/docker.sock:/var/run/docker.sock:ro
```