# xmtr

Trace several targets at once and see every hop on a map.

`xmtr` runs concurrent traceroutes with [trippy](https://github.com/fujiapple852/trippy), stops each route as soon as its path is stable, and uploads the result to [xmsl.dev IPMap](https://xmsl.dev/ipmap). IPMap places every hop on a map and replays the route.

![IPMap showing a route from Rennes through London, the United States and Tokyo to Guam](https://raw.githubusercontent.com/Alice39s/xmtr/main/.github/assets/ipmap-demo.png)

```sh
sudo xmtr 1.1.1.1 example.com
```

xmtr prints each route as a table, then the trace ID and the link to its map.

## Privileges

Tracing needs raw sockets.

| System | Run as |
| --- | --- |
| Linux | root, or grant `cap_net_raw` to the binary once |
| macOS | any user; ICMP and UDP use unprivileged sockets, TCP needs root |
| Windows | an Administrator terminal |
| FreeBSD, Android | root |

## More

Install options, every command-line flag and the upload format are in the [README on GitHub](https://github.com/Alice39s/xmtr#readme). Update xmtr with the package manager that installed it rather than `xmtr self update`.

xmtr is licensed under either of MIT or Apache-2.0, at your option.
