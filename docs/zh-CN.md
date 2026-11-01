<h1 align="center">xmtr</h1>

<p align="center">
  同时追踪多个目标，在地图上查看每一跳。
</p>

<p align="center">
  <a href="https://github.com/Alice39s/xmtr/releases/latest"><img alt="Release" src="https://img.shields.io/github/v/release/Alice39s/xmtr?style=flat-square"></a>
  <a href="https://github.com/Alice39s/xmtr/actions/workflows/release.yml"><img alt="Build" src="https://img.shields.io/github/actions/workflow/status/Alice39s/xmtr/release.yml?branch=main&style=flat-square"></a>
  <img alt="License" src="https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue?style=flat-square">
</p>

<p align="center">
  <a href="../README.md">English</a> | <a href="zh-CN.md">简体中文</a> | <a href="zh-TW.md">繁體中文（台灣）</a> | <a href="zh-HK.md">繁體中文（香港）</a> | <a href="zh-MO.md">繁體中文（澳門）</a>
</p>

`xmtr` 用 [trippy](https://github.com/fujiapple852/trippy) 同时追踪多个目标，每条路由的路径稳定后就停止，然后把路由上传到 [xmsl.dev IPMap](https://xmsl.dev/ipmap)。IPMap 把每一跳标在地图上，并回放整条路由。

https://github.com/user-attachments/assets/b2152647-3396-4f2c-86d4-267c2b11e529

运行结束时给出的链接会打开这张地图：

https://github.com/user-attachments/assets/b675a8f5-dff1-460a-bb3c-ca7b3e4ff412

## 快速上手

```sh
curl -fsSL https://raw.githubusercontent.com/Alice39s/xmtr/main/install.sh | bash
sudo xmtr 1.1.1.1 cloudflare.com
```

第二条命令追踪 1.1.1.1 和 cloudflare.com，主机同时有 IPv4 和 IPv6 时两种都追踪，最后输出路由表和 IPMap 链接。Windows 与各包管理器的安装方式见[安装](#安装)，各系统要以什么身份运行见[运行身份](#运行身份)。

## 安装

### 安装脚本

**macOS、Linux、FreeBSD、Termux**

```sh
curl -fsSL https://raw.githubusercontent.com/Alice39s/xmtr/main/install.sh | bash
```

脚本只用 POSIX `sh` 语法，没有 bash 的系统（例如 OpenWrt）可以改用 `| sh`。普通用户运行时，脚本通过 `sudo` 安装到 `/usr/local/bin`，这样 `sudo xmtr` 能找到它；sudo 可能会要求输入密码。不能使用 sudo 时安装到 `~/.local/bin`。

**Windows**

```powershell
powershell -c "irm https://raw.githubusercontent.com/Alice39s/xmtr/main/install.ps1|iex"
```

两个脚本都会挑出适合本机的发布文件，用发布附带的 `SHA256SUMS` 校验，再运行一个小安装器。安装器放好唯一的 `xmtr` 可执行文件后删除自身。除了 Windows 上当前用户的 `PATH` 条目，不会写入其他内容。下载走 HTTPS，只有 `XMTR_RELEASES` 指向 `http://` 镜像时例外。

| 系统 | 默认位置 |
| --- | --- |
| Linux、macOS、FreeBSD | `/usr/local/bin`；不能使用 sudo 时为 `~/.local/bin` |
| Termux | `$PREFIX/bin` |
| Windows | `%LOCALAPPDATA%\Programs\xmtr` |

安装器的选项写在 `bash -s --` 后面：

```sh
curl -fsSL https://raw.githubusercontent.com/Alice39s/xmtr/main/install.sh | bash -s -- --dir ~/bin --version v0.3.4
```

`XMTR_INSTALL_DIR` 设置安装目录，`XMTR_RELEASES` 让脚本和安装器都从 releases 页面的镜像下载。在 Windows 上只能用这两个环境变量传选项。

### 包管理器

| | 包管理器 | 命令 |
| :-: | --- | --- |
| <img src="../.github/assets/homebrew.svg" width="20" alt=""> | Homebrew（macOS、Linux） | `brew install alice39s/tap/xmtr` |
| <img src="../.github/assets/scoop.png" width="20" alt=""> | Scoop（Windows） | 先 `scoop bucket add alice39s https://github.com/Alice39s/scoop-bucket`，再 `scoop install alice39s/xmtr` |
| <img src="../.github/assets/windows.svg" width="20" alt=""> | WinGet（Windows） | `winget install Alice39s.xmtr` |
| <img src="../.github/assets/chocolatey.svg" width="20" alt=""> | Chocolatey（Windows） | `choco install xmtr` |
| <img src="../.github/assets/npm.svg" width="20" alt=""> | npm | `npm install --global @xmtr/cli`，或 `npx @xmtr/cli 1.1.1.1` |
| <img src="../.github/assets/pypi.svg" width="20" alt=""> | PyPI | `uv tool install xmtr`、`pipx install xmtr` 或 `pip install xmtr` |
| <picture><source media="(prefers-color-scheme: dark)" srcset="../.github/assets/rust-dark.svg"><img src="../.github/assets/rust.svg" width="20" alt=""></picture> | Cargo | `cargo binstall xmtr` 下载发布好的可执行文件，`cargo install xmtr` 从源码编译 |
| <img src="../.github/assets/mise.svg" width="20" alt=""> | mise | `mise use --global github:Alice39s/xmtr` |
| <img src="../.github/assets/nixos.svg" width="20" alt=""> | Nix | `nix run github:Alice39s/xmtr -- 1.1.1.1`，或 `nix profile install github:Alice39s/xmtr` |
| <img src="../.github/assets/docker.svg" width="20" alt=""> | Docker | `docker run --rm ghcr.io/alice39s/xmtr 1.1.1.1` |

npm 包和 PyPI wheel 里的可执行文件与 release 压缩包里的相同；npm 会按本机系统选用对应的 `@xmtr/<platform>` 包。Docker 镜像里只有静态链接的可执行文件，提供 amd64、arm64、armv7、armv6、386、riscv64 和 loong64 版本。Docker 默认允许容器使用原始套接字；Podman 需要加 `--cap-add NET_RAW`。

### Linux 软件源

deb、rpm 和 apk 包把程序安装为 `/usr/bin/xmtr`，并用 `setcap` 授予原始套接字能力，所有用户不用 root 就能追踪。这些包附在每个 release 上，也由带签名的软件源提供：

| | 系统 | 架构 |
| :-: | --- | --- |
| <img src="../.github/assets/debian.svg" width="20" alt=""> | Debian、Ubuntu | amd64、arm64、armhf、i386、riscv64、loong64 |
| <img src="../.github/assets/fedora.svg" width="20" alt=""> | Fedora、RHEL | x86_64、aarch64、armv7hl、i686、riscv64、loongarch64 |
| <img src="../.github/assets/alpinelinux.svg" width="20" alt=""> | Alpine | x86_64、aarch64、armv7、armhf、x86、riscv64、loongarch64 |

```sh
# Debian、Ubuntu
sudo curl -fsSL -o /usr/share/keyrings/xmtr.gpg https://alice39s.github.io/xmtr/xmtr.gpg
echo "deb [signed-by=/usr/share/keyrings/xmtr.gpg] https://alice39s.github.io/xmtr/deb stable main" | sudo tee /etc/apt/sources.list.d/xmtr.list
sudo apt update && sudo apt install xmtr

# Fedora、RHEL
sudo curl -fsSL -o /etc/yum.repos.d/xmtr.repo https://alice39s.github.io/xmtr/rpm/xmtr.repo
sudo dnf install xmtr

# Alpine
wget -qO /etc/apk/keys/xmtr.rsa.pub https://alice39s.github.io/xmtr/apk/xmtr.rsa.pub
echo https://alice39s.github.io/xmtr/apk >> /etc/apk/repositories
apk add xmtr
```

软件源的签名密钥在 [`packaging/keys`](../packaging/keys)。上面所有渠道都由发布流水线推送，mise 和 Nix 除外，它们直接读取 GitHub release 和 flake。

### 更新与卸载

```sh
xmtr self update              # 用最新版本覆盖当前程序
xmtr self update --tag v0.3.3 # 安装指定版本
xmtr self uninstall           # 删除当前程序
```

`--force` 会重新安装当前已是最新的版本。通过包管理器或 Linux 软件源安装的，请用对应的包管理器更新。

在终端里运行结束后，xmtr 会查询最新版本，最多等 3 秒。有新版本时，它先问要不要安装，再问要不要用新版本重新运行同一条命令；两次都直接按 Enter 表示同意。如果程序由包管理器管理，或者所在目录不可写，xmtr 改为输出对应的更新命令。带 `--json`、`--quiet` 或 `--no-upload` 的运行，以及 stdin 和 stderr 不是终端的运行，都不会查询。

### 校验下载的文件

手动安装用的压缩包在 [releases 页面](https://github.com/Alice39s/xmtr/releases)。release 不可修改：发布之后，其中的文件和 tag 都不能再变。

```sh
gh release verify-asset <tag> <file> --repo Alice39s/xmtr   # 文件属于该 release
gh attestation verify <file> --repo Alice39s/xmtr           # 文件由发布流水线构建
```

第二条命令检查的是[构建来源证明](https://github.com/Alice39s/xmtr/attestations)。

## 支持的平台

| | 系统 | 架构 |
| :-: | --- | --- |
| <img src="../.github/assets/linux.svg" width="20" alt=""> | Linux，glibc 与 musl | <img src="../.github/assets/amd.svg" width="14" alt=""> x86_64 · <img src="../.github/assets/intel.svg" width="14" alt=""> i686 · <img src="../.github/assets/arm.svg" width="14" alt=""> aarch64、armv7、armv6 · <img src="../.github/assets/riscv.svg" width="14" alt=""> riscv64 · <img src="../.github/assets/chip.svg" width="14" alt=""> loongarch64 |
| <img src="../.github/assets/linux.svg" width="20" alt=""> | Linux，glibc | <img src="../.github/assets/ibm.svg" width="24" alt=""> powerpc64le、powerpc64、s390x · <img src="../.github/assets/oracle.svg" width="14" alt=""> sparc64 |
| <picture><source media="(prefers-color-scheme: dark)" srcset="../.github/assets/apple-dark.svg"><img src="../.github/assets/apple.svg" width="20" alt=""></picture> | macOS | <img src="../.github/assets/arm.svg" width="14" alt=""> Apple silicon · <img src="../.github/assets/intel.svg" width="14" alt=""> x86_64 |
| <img src="../.github/assets/windows.svg" width="20" alt=""> | Windows 10 及以上 | <img src="../.github/assets/amd.svg" width="14" alt=""> x86_64 · <img src="../.github/assets/intel.svg" width="14" alt=""> i686 · <img src="../.github/assets/arm.svg" width="14" alt=""> ARM64 |
| <img src="../.github/assets/freebsd.svg" width="20" alt=""> | FreeBSD | <img src="../.github/assets/amd.svg" width="14" alt=""> x86_64 |
| <img src="../.github/assets/android.svg" width="20" alt=""> | Android，需要 root 的 Termux | <img src="../.github/assets/arm.svg" width="14" alt=""> aarch64、armv7 · <img src="../.github/assets/amd.svg" width="14" alt=""> x86_64 · <img src="../.github/assets/intel.svg" width="14" alt=""> i686 |

所有版本都在 Linux 上交叉编译。每次发布前，CI 会运行每一个版本并用它追踪 `127.0.0.1`：Linux、macOS 和 Windows 的 x86_64 与 ARM64 版本跑在托管 runner 上，其余 Linux CPU 架构跑在 QEMU 用户模式模拟里，FreeBSD 跑在虚拟机里，Android 跑在 Termux 镜像里。发布之后，CI 在 Linux、macOS 和 Windows runner 上通过每个包管理渠道安装一遍 xmtr。

| | 系统 | 没有版本的原因 |
| :-: | --- | --- |
| <img src="../.github/assets/netbsd.svg" width="20" alt=""> | NetBSD | trippy 的套接字层在该系统上无法编译 |
| <img src="../.github/assets/openbsd.svg" width="20" alt=""> | OpenBSD、DragonFly BSD | Rust 没有为交叉编译提供预编译的标准库 |
| <img src="../.github/assets/chip.svg" width="20" alt=""> | illumos、Solaris | trippy 的套接字层在该系统上无法编译 |
| <img src="../.github/assets/webassembly.svg" width="20" alt=""> | iOS、WebAssembly | 命令行程序无法使用原始套接字 |

## 运行身份

追踪需要原始套接字。

| 系统 | 以什么身份运行 |
| --- | --- |
| Linux | root；或者执行一次 `sudo setcap cap_net_raw+ep $(command -v xmtr)`，deb、rpm 和 apk 包安装时已经做过这一步 |
| macOS | 任意用户；ICMP 和 UDP 使用非特权套接字，TCP 需要 root |
| Windows | 管理员终端 |
| FreeBSD、Android | root |

`xmtr self update` 会替换可执行文件，所以更新后要重新执行一次 `setcap`。

## 用法

```sh
xmtr 1.1.1.1 cloudflare.com 9.9.9.9   # 同时追踪三个目标
xmtr -4 --min-rounds 20 example.com    # 只追踪 IPv4，至少跑 20 轮
xmtr -p tcp --port 443 example.com     # 用 TCP 探测 443 端口
xmtr --no-upload 192.0.2.1             # 只输出路由，不上传
xmtr --pin 2580 example.com            # 打开地图前需要输入 2580
xmtr -q 1.1.1.1                        # 只输出 IPMap 链接
xmtr --json 1.1.1.1 > trace.json       # 以 JSON 输出上传内容，表格写到 stderr
```

全部选项见[选项](#选项)。

### 一次运行的过程

一次运行分为五步，进度面板里的编号与下面相同。

1. **Resolve（解析）**：解析每个目标。同时有 A 和 AAAA 记录的主机名会同时走 IPv4 和 IPv6 追踪，每种地址族按解析器返回的顺序取第一个地址。本机没有路由的地址族会被跳过。
2. **Trace（追踪）**：最多 8 条路由并行（`-j`）。每个追踪器有各自的 ICMP ID、UDP 源端口和序列号区间，不同路由的回包不会混在一起。一条路由跑满 `--min-rounds` 轮、连续 3 轮没有出现新的跳、并且目标有回应后就停止。始终没有到达目标的路由会多跑 5 轮。`--max-rounds` 和 `--time-limit` 是硬上限，追踪器跑的轮数不会超过时间限制内能跑完的轮数。

   使用 `--protocol auto` 时，如果 ICMP 跑了 4 轮还没到达目标，xmtr 会再用 TCP 80 端口追踪一次；使用非特权套接字时改用 UDP。第二条路由与 ICMP 路由同时进行，在进度面板里单独占一行，也计入 `-j`；如果 ICMP 路由提前结束且没有到达目标，第二条路由在它结束时开始。两条路由都会上传。没有任何一跳回应的路由会作为错误报告，不上传。

   通过原始套接字发送的 UDP 探测采用 Paris 方式：同一轮的所有探测包端口和 UDP 校验和都相同，在多条并行链路之间按流分担的路由器会把整轮探测发到同一条链路上，每一轮显示的都是一条真实路径。ICMP、TCP 以及非特权套接字的 UDP 使用经典探测包。
3. **Lookup（查询）**：为公网地址的跳查询反向解析名称和所属 AS 号（[Team Cymru](https://www.team-cymru.com/ip-asn-mapping)），同时最多查询 32 个地址。这一步最多持续 5 秒，结果里会注明有多少个地址没有回应。私有地址永远不会发给 `--dns` 指定的解析器。`--no-rdns` 跳过这一步。
4. **Upload（上传）**：把路由上传到 `--api`，详见[上传与分享](#上传与分享)。`--no-upload` 跳过这一步。
5. **Result（结果）**：输出 IPMap 链接、备注，以及每条路由的表格。

按 Ctrl-C 会终止运行，退出码为 130。

### 进度与输出

在终端里，提示符下方的面板会实时显示各个步骤：当前进行到哪一步、每条路由一根进度条，以及已有多少个反向解析得到回应。运行结束后，面板变成结果：先是 IPMap 链接，然后是备注，接着每条路由一张表格，连续不回应的跳折叠成一行。面板画在 stderr 上，同时要求 stdout 也连着终端。

`--progress` 选择进度视图，`auto` 根据终端自动选择：

| 视图 | `auto` 何时选它 | 显示内容 |
| --- | --- | --- |
| `panel` | stdout 和 stderr 都连着终端，终端至少 44 列、8 行，并且能回报光标位置 | 上面描述的面板 |
| `line` | stdout 被重定向，或者终端更小、不能回报光标位置 | 已完成的步骤和路由各占一行，下方一行状态，最后输出表格和链接 |
| `plain` | stderr 不是终端，或者 `TERM=dumb` | 每个步骤和每条完成的路由各一行，例如 `[2/5] 1.1.1.1 v4 ICMP: reached, 7 rounds, 5.2s`；没有变化时每 10 秒输出一次当前状态 |

如果路由器在 MPLS 隧道里转发了探测包，并在回包里带上标签栈（[RFC 4950](https://www.rfc-editor.org/rfc/rfc4950)），它的地址下方会每个标签占一行，例如 `[MPLS: Lbl 24001 TC 0 S 1 TTL 1]`，与 `mtr -e` 的格式相同。隧道里每个路由器报告的往返时间都接近隧道最后一个路由器的往返时间，因为它的回包要先走到隧道终点再折返。

表格放不下终端宽度时，即使把长主机名换行也放不下，就改成每跳一行，只列主机、平均往返时间和丢包。`--quiet` 关闭进度显示，只输出链接。

`--theme` 设置面板和行视图的配色。`auto` 会向终端查询背景色，`COLORTERM` 为 `truecolor` 或 `24bit` 时使用 RGB 颜色；`dark`、`light`、`dark-ansi` 和 `light-ansi` 直接指定。

通过管道或重定向输出时，xmtr 以纯文本输出每条路由，最后是 trace ID 和链接：

```text
$ sudo xmtr --min-rounds 20 121.55.201.201 | cat
121.55.201.201 -> 121.55.201.201 [ICMP IPv4] 20 rounds, 20.4s, reached
┌─────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┐
│ Hop   Host                                                               ASN        Loss%   Snt    Last     Avg    Best    Wrst   StDev │
╞═════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════╡
│   1   dgw.vyme.net (45.88.180.254)                                       AS199297     0.0    20     0.4     0.4     0.3     0.9     0.1 │
│   2   te0-0-0-2.nr61.b074095-0.rns01.atlas.cogentco.com (149.6.72.106)   AS174        0.0    20     1.3     1.1     0.8     1.6     0.2 │
│   3   be2928.rcr51.rns01.atlas.cogentco.com (154.25.12.25)               AS174        0.0    20     1.5     1.6     1.5     2.0     0.1 │
│   4   be7070.rcr61.uro01.atlas.cogentco.com (154.54.76.85)               AS174        0.0    20     6.0     5.7     5.5     6.1     0.2 │
│   5   be2480.ccr42.par01.atlas.cogentco.com (130.117.3.13)               AS174        0.0    20     7.3    10.9     7.0    44.4    11.2 │
│   6   port-channel2732.ccr92.lhr01.atlas.cogentco.com (154.54.75.138)    AS174        0.0    20    14.1    14.1    14.0    14.5     0.1 │
│   7   be3467.ccr32.bos01.atlas.cogentco.com (154.54.94.45)               AS174        0.0    20    76.4    76.3    76.2    76.5     0.1 │
│   8   be8030.ccr22.alb02.atlas.cogentco.com (154.54.169.226)             AS174        0.0    20    79.9    79.9    79.7    80.6     0.2 │
│   9   port-channel8023.ccr92.cle04.atlas.cogentco.com (154.54.169.197)   AS174        0.0    20    87.4    87.5    87.3    87.6     0.1 │
│  10   be2718.ccr42.ord01.atlas.cogentco.com (154.54.7.129)               AS174        0.0    20    99.3    99.0    98.8    99.3     0.1 │
│  11   be5068.ccr32.oma02.atlas.cogentco.com (154.54.166.73)              AS174        0.0    20   102.7   103.6   102.7   118.3     3.5 │
│  12   be8568.ccr82.den01.atlas.cogentco.com (154.54.95.109)              AS174       70.0    20   114.7   114.2   113.8   114.7     0.3 │
│  13   be9563.ccr82.slc03.atlas.cogentco.com (154.54.163.206)             AS174       55.0    20   125.1   124.5   123.5   125.1     0.7 │
│  14   be3906.ccr22.sfo01.atlas.cogentco.com (154.54.5.154)               AS174        0.0    20   138.5   138.6   138.5   138.9     0.1 │
│  15   be3670.ccr41.sjc03.atlas.cogentco.com (154.54.43.14)               AS174        0.0    19   140.7   140.6   140.4   140.8     0.1 │
│  16   38.104.138.31                                                      AS174        0.0    19   140.0   140.1   139.8   140.3     0.1 │
│  17   tky009bb10.IIJ.Net (58.138.88.225)                                 AS2497      73.7    19   240.2   241.9   240.2   242.9     1.1 │
│  18   tky009ix55.IIJ.Net (58.138.112.109)                                AS2497       0.0    19   241.4   241.3   241.2   241.4     0.1 │
│  19   210.173.173.82                                                     -            0.0    19   239.1   239.0   238.9   239.2     0.1 │
│  20   198.81.233.103.guam.net (198.81.233.103)                           AS3605       0.0    19   235.2   235.3   235.0   235.5     0.1 │
│  21   202.128.5.115.guam.net (202.128.5.115)                             AS3605       0.0    19   270.3   270.2   270.0   270.7     0.2 │
│  22   ae14-cgw-har-gu.guam.net (198.81.233.92)                           AS3605       0.0    19   264.4   264.5   264.1   266.4     0.5 │
│  23   ???                                                                -          100.0    19       -       -       -       -       - │
│  24   ???                                                                -          100.0    19       -       -       -       -       - │
│  25   121-55-201-201.d.c200.guam.net (121.55.201.201)                    AS3605       0.0    19   283.7   282.6   279.6   285.2     1.9 │
└─────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┘

trace-id 103115083070058496-nBjPEnny
https://xmsl.dev/xmtr/103115083070058496-nBjPEnny
```

### 上传与分享

上传成功后会返回 trace ID 和地图链接。trace ID 由一个 snowflake ID 和 8 个字母的密钥组成。拿到链接的人都能打开这条记录，密钥让别人无法猜出链接。

| 选项 | 作用 |
| --- | --- |
| `--pin 2580` | 打开链接后要输入这个四位 PIN（数字 0-8，不含 4）才能查看 |
| `--hide-origin` | 上传时去掉源地址；前几跳仍可能暴露出发的网络 |
| `--no-upload` | 路由只留在本机，不上传 |

IPMap 把探测包离开时的公网地址作为每条路由的起点。如果本地地址不是公网地址（私有地址、CGNAT、ULA、链路本地地址或其他保留地址），xmtr 会从这个本地地址直接向网关发一个 HTTPS 请求，不经过代理设置，取网关看到的地址。这个请求在追踪期间进行，最多等 3 秒；拿不到公网源地址的路由上传时不带源地址。

上传内容中的源地址、目的地址，以及以 IP 地址形式给出的目标，都会截断到所在网段：IPv4 取 /24，IPv6 取 /48，例如 `203.0.113.7` 变成 `203.0.113.0`，`2001:db8:1:2::7` 变成 `2001:db8:1::`。各跳保留完整地址，终端里的表格也显示完整的目的地址。每跳的每个地址还附带它自己回包中最短的往返时间，以及它带上的 MPLS 标签栈。

### DNS

`--dns` 指定用于解析目标名称、上传、反向解析和 AS 查询的解析器。

| 取值 | 解析器 |
| --- | --- |
| `auto` | 系统配置；目标名称交给操作系统解析，hosts 文件生效 |
| <img src="../.github/assets/cloudflare.svg" width="14" alt=""> `cloudflare`、<img src="../.github/assets/google.svg" width="14" alt=""> `google`、<img src="../.github/assets/quad9.svg" width="14" alt=""> `quad9`、`9.9.9.9`、`192.0.2.53:5353` | 基于 UDP 和 TCP 的普通 DNS |
| `tcp://[2620:fe::fe]` | DNS over TCP |
| `tls://[2606:4700:4700::1111]`、`dot://dns.quad9.net` | DNS over TLS |
| `https://cloudflare`、`doh://dns.google/dns-query` | DNS over HTTPS |
| `quic://dns.adguard-dns.com` | DNS over QUIC |

`cloudflare`、`google` 和 `quad9` 也可以用作 URL 的主机部分，并且同时包含 IPv4 和 IPv6 服务器。URL 里的主机名用系统解析器解析一次；证书按公共根证书校验。DNS over HTTPS 需要主机名或 IPv4 地址。

## 选项

```text
用法: xmtr [OPTIONS] <TARGET>...
      xmtr self update [--force] [--tag <TAG>]
      xmtr self uninstall

-p, --protocol <auto|icmp|udp|tcp>   探测协议（默认 auto）
-4, --ipv4 / -6, --ipv6              只追踪一种地址族
    --min-rounds <N>                 路由可以停止前至少跑的轮数（默认 5）
-c, --max-rounds <N>                 轮数上限（默认 30）
-t, --time-limit <DURATION>          每条路由的时间上限，至少 1s 且大于 -i（默认 45s）
-i, --interval <DURATION>            两轮之间的最短间隔，至少 100ms（默认 1s）
-m, --max-ttl <N>                    最大 TTL（默认 30）
    --port <PORT>                    -p tcp 的目的端口（默认 443）
-j, --parallel <N>                   同时追踪的路由数（默认等于路由总数，最多 8）
-u, --unprivileged                   使用非特权套接字（macOS）
    --dns <RESOLVER>                 见上文 DNS 一节（默认 auto）
    --no-rdns                        跳过反向解析和 AS 查询
    --no-upload                      只输出路由
    --hide-origin                    上传时不带路由的源地址
    --pin <PIN>                      四位数字，取 0-8，不含 4
    --api <URL>                      上传网关，也可用 XMTR_API 设置
    --json                           在 stdout 输出上传内容，表格写到 stderr
-q, --quiet                          只输出 IPMap 链接
    --progress <VIEW>                auto、panel、line 或 plain（默认 auto）
    --theme <THEME>                  auto、dark、light、dark-ansi 或 light-ansi（默认 auto）
```

## 遥测

发布版本会向 [GlitchTip](https://glitchtip.com) 上的 xmtr 项目发送报告。从源码构建的版本（例如 `cargo install` 和 `nix build`）不发送任何内容，除非编译时设置了 `XMTR_TELEMETRY_DSN`。

| 报告 | 何时发送 | 内容 |
| --- | --- | --- |
| 使用情况 | 每 100 次带上传的运行抽一次，以及每次 `xmtr self` 命令 | xmtr 版本、操作系统及版本（例如 Debian 13）、CPU 架构、命令、耗时、这次运行是否出错 |
| 错误 | 运行出错后，你回答 yes 时 | 错误信息、xmtr 版本、操作系统、CPU 架构 |
| 崩溃 | 发生 panic 后，你回答 yes 时 | panic 信息及其源码位置、xmtr 版本、操作系统、CPU 架构 |

只有 stdin 和 stderr 都是终端，并且没有设置 `--json` 或 `--quiet` 时，xmtr 才会询问是否发送错误和崩溃报告。默认回答是 no，没有终端时什么都不发送。发送前，xmtr 把 IP 地址替换成 `[ip]`；把目标、以主机名给出的解析器或网关、代理主机替换成 `[host]`；把用户主目录替换成 `~`；把路径里的用户名替换成 `[user]`；并去掉 URL 里的凭据。报告不包含本机名称和命令行。GlitchTip 会记录报告的来源地址，并截断到网络前缀。

`DO_NOT_TRACK=1` 关闭所有报告和询问（[Console Do Not Track](https://consoledonottrack.com)）。

## 从源码构建

```sh
cargo build --release --workspace
```

工作区会构建 `xmtr` 和 `xmtr-installer`，需要 Rust 1.88 或更新版本。把 `XMTR_TELEMETRY_DSN` 设为 GlitchTip 或 Sentry 的 DSN，构建出的程序就带[遥测](#遥测)。`nix build` 从 flake 构建 `xmtr`。

发布构建用 [cargo-zigbuild](https://github.com/rust-cross/cargo-zigbuild) 编译 Linux、Windows 和 macOS 版本，用 [cross](https://github.com/cross-rs/cross) 编译 FreeBSD、powerpc64 和 sparc64 版本，用 [cargo-ndk](https://github.com/bbqsrc/cargo-ndk) 编译 Android 版本；见 `.github/workflows/release.yml` 和 [`scripts/ci`](../scripts/ci/README.md)。工作区使用 `vendor/` 里打过补丁的 trippy-core，让 Android 能追踪 IPv4；见 [vendor/README.md](../vendor/README.md)。

## 翻译

本文是 README 的简体中文版。繁体中文版由 [OpenCC](https://github.com/BYVoid/OpenCC) 从本文生成：`zh-TW.md` 使用台湾用词（`s2twp`），`zh-HK.md` 和 `zh-MO.md` 使用香港字形（`s2hk`）。修改 `zh-CN.md` 后重新生成：

```sh
uv run --project scripts/ci xmtr-ci docs
```

## 发布

发布由 [knope](https://github.com/knope-dev/knope) 和变更文件管理：

1. 每个用户能感知到的改动，都运行 `knope document-change`，并提交它写入 `.changeset/` 的文件。
2. 发布时运行 `knope release`。它更新 `Cargo.toml`、`Cargo.lock`、README 及其各语言版本中的版本号，把变更文件合并进 [CHANGELOG.md](../CHANGELOG.md)，提交改动，再打上 `vX.Y.Z` tag。1.0 之前，`minor` 级别的变更文件只增加 patch 版本号；`knope release --override-version X.Y.0` 可以直接指定版本。
3. `git push --atomic origin main vX.Y.Z` 触发发布流水线。它构建并检查所有目标平台，发布 GitHub release（说明取自 CHANGELOG.md 中对应的一节），然后发布到各个包管理器。

CI 会检查 `knope.toml`，确认所有带版本号的文件版本一致，并在还有待发布的变更文件时，把下一个版本的预览写进任务摘要。

## 许可证

按你的选择，使用 [Apache License 2.0](../LICENSE-APACHE) 或 [MIT](../LICENSE-MIT) 许可证。
