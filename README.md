# Docker Proxy Filter

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Docker Pulls](https://img.shields.io/docker/pulls/foxxmd/docker-proxy-filter)](https://hub.docker.com/r/foxxmd/docker-proxy-filter)

Docker Proxy Filter (DPF) is a smol, forward proxy for **filtering the _content_ and _responses_** of Docker API responses to only those you want to expose.

Unlike the OG [docker-socket-proxy](https://github.com/Tecnativa/docker-socket-proxy) and its variants, DPF provides filtering of the _response content_ from the Docker API, rather than disabling/enabling of API endpoints.

It does not connect directly to the Docker socket: it designed to be used with another Docker "Socket Proxy" container.

Combined with a socket-proxy container that provides granular endpoint access it's possible to expose only information about specific containers in a read-only context.

## Features

### `CONTAINER_NAMES`

Using this ENV changes Docker API responses:

* Filters [List Containers](https://docs.docker.com/reference/api/engine/version/v1.48/#tag/Container/operation/ContainerList) responses so any container with a name that does not include a value from `CONTAINER_NAMES` is removed.
* Any other [Container](https://docs.docker.com/reference/api/engine/version/v1.48/#tag/Container) endpoints will return 404 if the container name does not include a value from `CONTAINER_NAMES`

### `SCRUB_ENVS`

When `true` any responses from the [Container Inspect](https://docs.docker.com/reference/api/engine/version/v1.48/#tag/Container/operation/ContainerInspect) endpoint will have `Config.Env` set to an empty array.

## Example

```yaml
services:
  proxy-filter:
    image: foxxmd/docker-proxy-filter
    environment:
      - PROXY_URL=http://socket-proxy:2375
      - CONTAINER_NAME=foo,bar
      - SCRUB_ENVS=true
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
      - '-allowGET=/_ping|/(v1\..{1,2}/)?(info|version|containers|events).*'
      - '-listenip=0.0.0.0'
      - '-allowfrom=proxy-filter'
      - '-stoponwatchdog'
      - '-shutdowngracetime=5'
    volumes:
      - /var/run/docker.sock:/var/run/docker.sock:ro
```

On your machine you are running these containers:


|  Id  |     Name     |
|------|--------------|
| 1234 | foo          |
| abcd | bar          |
| 6969 | cool-program |
| 0444 | fun-program  |

```shell
$ curl -i http://localhost:2375/v1.47/containers
HTTP/1.1 200 OK
content-length: 1234
content-type: application/json
date: Wed, 08 Oct 2025 00:33:02 GMT

[{"Id": 1234, "Names": ["/foo"] ...},{"Id": "abcd": "Names": ["/bar"]}]
# cool-program and fun-program have been filtered out of array
```

```shell
$ curl -i http://localhost:2375/v1.47/containers/6969/json
HTTP/1.1 404 Not Found
transfer-encoding: chunked
content-type: application/json
date: Wed, 08 Oct 2025 00:30:54 GMT

{"message":"No such container: 6969"}
# returns 404 as if no container is running
```

```shell
$ curl -i http://localhost:2375/v1.47/containers/1234/json
HTTP/1.1 404 Not Found
transfer-encoding: chunked
content-type: application/json
date: Wed, 08 Oct 2025 00:30:54 GMT

{"Id": 1234, "Name": "/foo" ...}
# returns container because Name is substring of CONTAINER_NAME values
```

```shell
$ curl -i http://localhost:2375/v1.47/volumes
HTTP/1.1 403 Forbidden
transfer-encoding: chunked
content-type: text/plain
date: Wed, 08 Oct 2025 00:39:59 GMT

Forbidden
# not allowed by wollomatic/socket-proxy config
```

## Configuration

All configuration is done through environmental variables.


|        Key        | Required | Default |                                                                          Description                                                                          |
|-------------------|----------|---------|---------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `PROXY_URL`       | yes      |         | The fully-qualified URL to proxy API requests EX `http://socket-proxy:2375`                                                                                   |
| `CONTAINER_NAMES` | yes      |         | A comma-delimited list of values. Any container that contains any value as a substring will be allowed.                                                       |
| `SCRUB_ENVS`      | no       | false   | Remove `Env` list from [container inspect API](https://docs.docker.com/reference/api/engine/version/v1.48/#tag/Container/operation/ContainerInspect) response |
