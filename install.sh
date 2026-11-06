#!/bin/sh
# ChronoSync installer / uninstaller / status tool.
# POSIX sh; tested with Debian systemd, GentooPlayer OpenRC conventions and
# AudioSys/Buildroot's /audiosys + CONFIG networkfs.sh hook model.
set -u

VERSION="1.1.0"
PREFIX="/opt/chronosync"
AUDIO_BASE="/audiosys/chronosync"
SELF_DIR=$(CDPATH= cd "$(dirname "$0")" && pwd)
SELF_PATH="$SELF_DIR/$(basename "$0")"

ACTION="install"
BIND_HOST="0.0.0.0"
PORT="8686"
AUTH=""
BINARY=""
LOG_DIR=""
NO_START=0
DRY_RUN=0
PURGE=0
PLATFORM=""
CLUSTER_MODE="off"
CLUSTER_ID="chronosync"
CLUSTER_BIND="0.0.0.0"
CLUSTER_DISCOVERY_PORT="18686"
CLUSTER_TIME_PORT="18687"
CLUSTER_SECRET_FILE=""
CLUSTER_SERVER_ELIGIBLE="true"
CLUSTER_AUTO_APPLY="0"
CLUSTER_SEED=""

msg()  { printf '[chronosync] %s\n' "$*"; }
warn() { printf '[chronosync] 警告：%s\n' "$*" >&2; }
die()  { printf '[chronosync] 错误：%s\n' "$*" >&2; exit 1; }

usage() {
    cat <<EOF
ChronoSync v$VERSION 安装器

用法：
  sudo ./install.sh [install|uninstall|status] [选项]

操作：
  install       安装到 /opt/chronosync，并按系统类型安装启动服务（默认）
  uninstall     停止服务并卸载程序；默认保留日志
  status        显示安装状态

选项：
  --bind ADDR       Web 控制台监听地址，默认 0.0.0.0（局域网可访问）
  --port PORT       Web 控制台端口，默认 8686
  --auth USER:PASS  可选 HTTP Basic 认证；密码保存在 root-only 文件中
  --log-dir DIR     日志目录；默认 /var/log/chronosync（AudioSys 为 /audiosys/chronosync/logs）
  --binary PATH     指定 chronosync 二进制；默认使用脚本旁边的 ./chronosync
  --no-start        只安装，不立即启动/重启服务
  --dry-run         只显示检测结果和安装计划，不修改系统
  --purge           卸载时同时删除日志和 AudioSys 持久目录
  --cluster-mode MODE        off/server/client/auto（默认 off）
  --cluster-id ID            集群标识（默认 chronosync）
  --cluster-bind ADDR        集群 UDP 绑定地址（默认 0.0.0.0）
  --cluster-discovery-port N 发现端口（默认 18686）
  --cluster-time-port N      时间端口（默认 18687）
  --cluster-secret-file PATH 管理员预置密钥；省略时安装器生成
  --cluster-server-eligible true|false  是否可被选为 server（默认 true）
  --cluster-auto-apply       client 自动写入本机时间（默认关闭）
  --cluster-seed ADDR        额外 seed（ip:port，可逗号分隔）
  -h, --help        显示本帮助

支持自动适配：
  systemd（Debian/Ubuntu 等）、OpenRC（GentooPlayer 等）、
  procd（iStoreOS 等）、AudioSys/Buildroot（/audiosys + CONFIG 开机钩子）、
  BusyBox/SysV 风格 init.d。

安全提示：
  控制台默认监听 0.0.0.0，局域网可直接访问。请设置 --auth 并只在可信
  内网使用；HTTP Basic 认证不是传输加密。
EOF
}

while [ $# -gt 0 ]; do
    case "$1" in
        install|uninstall|status) ACTION="$1"; shift ;;
        --bind) [ $# -ge 2 ] || die "--bind 缺少参数"; BIND_HOST="$2"; shift 2 ;;
        --port) [ $# -ge 2 ] || die "--port 缺少参数"; PORT="$2"; shift 2 ;;
        --auth) [ $# -ge 2 ] || die "--auth 缺少参数"; AUTH="$2"; shift 2 ;;
        --log-dir) [ $# -ge 2 ] || die "--log-dir 缺少参数"; LOG_DIR="$2"; shift 2 ;;
        --binary) [ $# -ge 2 ] || die "--binary 缺少参数"; BINARY="$2"; shift 2 ;;
        --no-start) NO_START=1; shift ;;
        --dry-run) DRY_RUN=1; shift ;;
        --purge) PURGE=1; shift ;;
        --cluster-mode) [ $# -ge 2 ] || die "--cluster-mode 缺少参数"; CLUSTER_MODE=$(printf '%s' "$2" | tr 'A-Z' 'a-z'); shift 2 ;;
        --cluster-id) [ $# -ge 2 ] || die "--cluster-id 缺少参数"; CLUSTER_ID="$2"; shift 2 ;;
        --cluster-bind) [ $# -ge 2 ] || die "--cluster-bind 缺少参数"; CLUSTER_BIND="$2"; shift 2 ;;
        --cluster-discovery-port) [ $# -ge 2 ] || die "--cluster-discovery-port 缺少参数"; CLUSTER_DISCOVERY_PORT="$2"; shift 2 ;;
        --cluster-time-port) [ $# -ge 2 ] || die "--cluster-time-port 缺少参数"; CLUSTER_TIME_PORT="$2"; shift 2 ;;
        --cluster-secret-file) [ $# -ge 2 ] || die "--cluster-secret-file 缺少参数"; CLUSTER_SECRET_FILE="$2"; shift 2 ;;
        --cluster-server-eligible) [ $# -ge 2 ] || die "--cluster-server-eligible 缺少参数"; CLUSTER_SERVER_ELIGIBLE="$2"; shift 2 ;;
        --cluster-auto-apply) CLUSTER_AUTO_APPLY=1; shift ;;
        --cluster-seed) [ $# -ge 2 ] || die "--cluster-seed 缺少参数"; CLUSTER_SEED="$2"; shift 2 ;;
        -h|--help) usage; exit 0 ;;
        *) die "未知参数：$1（使用 --help 查看用法）" ;;
    esac
done

# ---- validation -----------------------------------------------------------
[ -n "$BIND_HOST" ] || die "监听地址不能为空"
case "$BIND_HOST" in
    *[!A-Za-z0-9.:_-]*) die "监听地址包含不支持的字符：$BIND_HOST" ;;
esac
case "$PORT" in
    ''|*[!0-9]*) die "端口必须是数字：$PORT" ;;
esac
[ "$PORT" -ge 1 ] 2>/dev/null && [ "$PORT" -le 65535 ] 2>/dev/null || die "端口超出范围：$PORT"

if [ -n "$AUTH" ]; then
    case "$AUTH" in
        *:*) ;;
        *) die "--auth 必须使用 user:password 格式" ;;
    esac
    case "$AUTH" in
        *'
'*) die "--auth 不能包含换行符" ;;
    esac
fi

case "$CLUSTER_MODE" in
    off|server|client|auto) ;;
    *) die "--cluster-mode 必须是 off/server/client/auto：$CLUSTER_MODE" ;;
esac
case "$CLUSTER_ID" in
    ''|*[!A-Za-z0-9._-]*) die "--cluster-id 仅允许字母、数字、点、下划线、短横线：$CLUSTER_ID" ;;
esac
[ ${#CLUSTER_ID} -le 64 ] || die "--cluster-id 过长"
case "$CLUSTER_BIND" in
    ''|*[!A-Za-z0-9.:_-]*) die "--cluster-bind 地址无效：$CLUSTER_BIND" ;;
esac
for cluster_port in "$CLUSTER_DISCOVERY_PORT" "$CLUSTER_TIME_PORT"; do
    case "$cluster_port" in
        ''|*[!0-9]*) die "集群端口必须是数字：$cluster_port" ;;
    esac
    [ "$cluster_port" -ge 1 ] 2>/dev/null && [ "$cluster_port" -le 65535 ] 2>/dev/null || die "集群端口超出范围：$cluster_port"
done
[ "$CLUSTER_DISCOVERY_PORT" != "$CLUSTER_TIME_PORT" ] || die "发现端口和时间端口不能相同"
case "$CLUSTER_SERVER_ELIGIBLE" in
    true|false) ;;
    *) die "--cluster-server-eligible 必须是 true 或 false" ;;
esac
if [ -n "$CLUSTER_SECRET_FILE" ] && [ ! -f "$CLUSTER_SECRET_FILE" ]; then
    die "集群密钥文件不存在：$CLUSTER_SECRET_FILE"
fi
case "$CLUSTER_SEED" in
    *[!A-Za-z0-9.,:_-]*) die "--cluster-seed 包含不支持的字符：$CLUSTER_SEED" ;;
esac
if [ "$CLUSTER_MODE" = "off" ] && [ "$CLUSTER_AUTO_APPLY" = "1" ]; then
    warn "集群模式为 off，忽略 --cluster-auto-apply"
    CLUSTER_AUTO_APPLY=0
fi
if [ "$CLUSTER_MODE" != "off" ] && [ ! -x "$BINARY" ] && [ "$DRY_RUN" != "1" ]; then
    :
fi

if [ -z "$BINARY" ]; then
    BINARY="$SELF_DIR/chronosync"
fi

# Restrict log paths to characters that can be embedded safely in generated
# shell scripts. All default paths comply.
if [ -n "$LOG_DIR" ]; then
    case "$LOG_DIR" in
        /*) ;;
        *) die "日志目录必须是绝对路径：$LOG_DIR" ;;
    esac
    case "$LOG_DIR" in
        *[!A-Za-z0-9/._-]*) die "日志目录包含不支持的字符（仅允许字母、数字、/ . _ -）：$LOG_DIR" ;;
    esac
fi

# ---- platform detection ---------------------------------------------------
detect_platform() {
    if [ -d /audiosys ] && [ -f /dpdk/diretta.sh ]; then
        PLATFORM="audiosys"
    elif [ -d /run/systemd/system ] && command -v systemctl >/dev/null 2>&1; then
        PLATFORM="systemd"
    elif command -v rc-update >/dev/null 2>&1 && command -v rc-service >/dev/null 2>&1; then
        PLATFORM="openrc"
    elif [ -x /sbin/procd ] && [ -f /etc/rc.common ]; then
        PLATFORM="procd"
    elif [ -d /etc/init.d ]; then
        PLATFORM="sysv"
    else
        PLATFORM="manual"
    fi
}

detect_platform
if [ -z "$LOG_DIR" ]; then
    if [ "$PLATFORM" = "audiosys" ]; then
        LOG_DIR="$AUDIO_BASE/logs"
    else
        LOG_DIR="/var/log/chronosync"
    fi
fi

platform_label() {
    case "$PLATFORM" in
        audiosys) printf 'AudioSys/Buildroot（/audiosys + CONFIG 开机钩子）' ;;
        systemd)  printf 'systemd' ;;
        openrc)   printf 'OpenRC' ;;
        procd)    printf 'procd（OpenWrt/iStoreOS）' ;;
        sysv)     printf 'BusyBox/SysV init.d' ;;
        *)        printf '未知（仅安装文件，不自动注册服务）' ;;
    esac
}

if [ "$DRY_RUN" = "1" ]; then
    msg "ChronoSync v$VERSION 安装计划（dry-run，不修改系统）"
    msg "系统类型 : $(platform_label)"
    msg "二进制   : $BINARY"
    msg "安装目录 : $PREFIX"
    msg "日志目录 : $LOG_DIR"
    msg "监听地址 : $BIND_HOST:$PORT"
    msg "访问认证 : $([ -n "$AUTH" ] && printf '已启用' || printf '未启用')"
    msg "集群模式 : $CLUSTER_MODE"
    if [ "$CLUSTER_MODE" != "off" ]; then
        msg "集群参数 : id=$CLUSTER_ID bind=$CLUSTER_BIND discovery=$CLUSTER_DISCOVERY_PORT/udp time=$CLUSTER_TIME_PORT/udp eligible=$CLUSTER_SERVER_ELIGIBLE"
        msg "集群密钥 : $([ -n "$CLUSTER_SECRET_FILE" ] && printf '管理员预置 %s' "$CLUSTER_SECRET_FILE" || printf '安装时生成')"
    fi
    case "$PLATFORM" in
        audiosys)
            msg "将安装 : $PREFIX、$AUDIO_BASE、CONFIG 分区 networkfs.sh 开机钩子、/usr/local/bin/chronosyncctl"
            ;;
        systemd) msg "将安装 : /etc/systemd/system/chronosync.service" ;;
        openrc)  msg "将安装 : /etc/init.d/chronosync（OpenRC）" ;;
        procd)   msg "将安装 : /etc/init.d/chronosync（procd）" ;;
        sysv)    msg "将安装 : /etc/init.d/S99chronosync" ;;
        *)       msg "将只安装程序文件；未检测到可自动注册的 init 系统" ;;
    esac
    if [ "$ACTION" = "install" ] && [ ! -x "$BINARY" ]; then
        warn "指定的二进制不存在或不可执行：$BINARY"
    fi
    exit 0
fi

[ "$(id -u)" = "0" ] || die "安装或卸载需要 root 权限，请使用 sudo 运行"

# ---- helpers --------------------------------------------------------------
self_install_copy() {
    dest="$1"
    [ -d "$dest" ] || return 0
    [ "$SELF_PATH" = "$dest/install.sh" ] && return 0
    cp -f "$SELF_PATH" "$dest/install.sh" 2>/dev/null || true
    chmod 755 "$dest/install.sh" 2>/dev/null || true
}

write_install_conf() {
    dest="$1"
    cat > "$dest/install.conf" <<EOF
PLATFORM='$PLATFORM'
BIND='$BIND_HOST'
PORT='$PORT'
LOG_DIR='$LOG_DIR'
CLUSTER_MODE='$CLUSTER_MODE'
CLUSTER_ID='$CLUSTER_ID'
CLUSTER_BIND='$CLUSTER_BIND'
CLUSTER_DISCOVERY_PORT='$CLUSTER_DISCOVERY_PORT'
CLUSTER_TIME_PORT='$CLUSTER_TIME_PORT'
CLUSTER_SERVER_ELIGIBLE='$CLUSTER_SERVER_ELIGIBLE'
CLUSTER_AUTO_APPLY='$CLUSTER_AUTO_APPLY'
CLUSTER_SEED='$CLUSTER_SEED'
EOF
    chmod 600 "$dest/install.conf"
}

write_run_script() {
    dest="$1"
    data_dir="${2:-$PREFIX}"
    cat > "$dest" <<EOF
#!/bin/sh
AUTH_FILE="$PREFIX/auth"
if [ -r "\$AUTH_FILE" ]; then
    CHRONOSYNC_AUTH="\$(cat "\$AUTH_FILE")"
    export CHRONOSYNC_AUTH
fi
set -- "$PREFIX/chronosync" --bind "$BIND_HOST" --port "$PORT" --log-dir "$LOG_DIR"
if [ "$CLUSTER_MODE" != "off" ]; then
    set -- "\$@" --cluster-mode "$CLUSTER_MODE" --cluster-id "$CLUSTER_ID" \
        --cluster-bind "$CLUSTER_BIND" --discovery-port "$CLUSTER_DISCOVERY_PORT" \
        --time-port "$CLUSTER_TIME_PORT" --server-eligible "$CLUSTER_SERVER_ELIGIBLE" \
        --cluster-secret-file "$data_dir/cluster.key" --cluster-state-dir "$data_dir/cluster-state"
    if [ "$CLUSTER_AUTO_APPLY" = "1" ]; then set -- "\$@" --auto-apply; fi
    if [ -n "$CLUSTER_SEED" ]; then set -- "\$@" --cluster-seed "$CLUSTER_SEED"; fi
fi
exec "\$@"
EOF
    chmod 755 "$dest"
}

write_auth() {
    dest="$1"
    if [ -n "$AUTH" ]; then
        printf '%s\n' "$AUTH" > "$dest" || die "写入认证文件失败：$dest"
        chmod 600 "$dest"
    else
        rm -f "$dest"
    fi
}

install_cluster_files() {
    dest="$1"
    key_src="${2:-}"
    if [ "$CLUSTER_MODE" = "off" ]; then
        rm -f "$dest/cluster.key"
        return 0
    fi
    mkdir -p "$dest/cluster-state" || die "无法创建集群状态目录：$dest/cluster-state"
    chmod 700 "$dest/cluster-state" 2>/dev/null || true
    if [ -n "$key_src" ] && [ -f "$key_src" ] && [ "$key_src" != "$dest/cluster.key" ]; then
        cp -f "$key_src" "$dest/cluster.key" || die "复制集群密钥失败：$dest/cluster.key"
    elif [ -n "$CLUSTER_SECRET_FILE" ] && [ "$CLUSTER_SECRET_FILE" != "$dest/cluster.key" ]; then
        cp -f "$CLUSTER_SECRET_FILE" "$dest/cluster.key" || die "复制管理员预置密钥失败"
    elif [ ! -s "$dest/cluster.key" ]; then
        if command -v od >/dev/null 2>&1; then
            dd if=/dev/urandom bs=32 count=1 2>/dev/null | od -An -tx1 | tr -d ' \n' > "$dest/cluster.key"
        elif command -v base64 >/dev/null 2>&1; then
            dd if=/dev/urandom bs=32 count=1 2>/dev/null | base64 | tr -d '\n' > "$dest/cluster.key"
        else
            die "无法生成集群密钥：缺少 od/base64；请使用 --cluster-secret-file"
        fi
    fi
    [ -s "$dest/cluster.key" ] || die "集群密钥为空：$dest/cluster.key"
    chmod 600 "$dest/cluster.key"
    if [ -z "$CLUSTER_SECRET_FILE" ] && [ -z "$key_src" ]; then
        warn "已在本机生成集群密钥：$dest/cluster.key"
        warn "集群其他节点必须使用同一密钥文件（--cluster-secret-file），否则无法通过 HMAC 认证"
    fi
}

install_binary_atomic() {
    src="$1"
    dest="$2"
    tmp="$dest.new.$$"
    cp -f "$src" "$tmp" || return 1
    chmod 755 "$tmp" || { rm -f "$tmp"; return 1; }
    mv -f "$tmp" "$dest" || { rm -f "$tmp"; return 1; }
    return 0
}

is_running_pidfile() {
    pidfile="$1"
    [ -f "$pidfile" ] || return 1
    pid=$(cat "$pidfile" 2>/dev/null)
    [ -n "$pid" ] || return 1
    kill -0 "$pid" 2>/dev/null
}

install_files() {
    mkdir -p "$PREFIX" "$LOG_DIR" || die "无法创建安装/日志目录"
    chmod 755 "$PREFIX"
    chmod 700 "$LOG_DIR"
    install_binary_atomic "$BINARY" "$PREFIX/chronosync" || die "复制二进制失败"
    self_install_copy "$PREFIX"
    write_run_script "$PREFIX/run.sh" "$PREFIX"
    write_auth "$PREFIX/auth"
    write_install_conf "$PREFIX"
    install_cluster_files "$PREFIX"
}

write_chronosyncctl() {
    dest="$1"
    cat > "$dest" <<EOF
#!/bin/sh
# AudioSys persistent control script. /opt is RAM-backed on AudioSys, so every
# start restores the binary from /audiosys/chronosync before launching it.
BASE="$AUDIO_BASE"
PREFIX="$PREFIX"
LOG_DIR="$LOG_DIR"
PIDFILE="/run/chronosync.pid"

start() {
    [ -x "\$BASE/chronosync" ] || { echo "ChronoSync binary is missing: \$BASE/chronosync" >&2; return 1; }
    [ -f "\$BASE/run.sh" ] || { echo "ChronoSync launcher is missing: \$BASE/run.sh" >&2; return 1; }
    mkdir -p "\$PREFIX" "\$LOG_DIR" /run || return 1
    mkdir -p /usr/local/bin 2>/dev/null
    ln -sf "\$BASE/chronosyncctl" /usr/local/bin/chronosyncctl 2>/dev/null
    if is_running; then return 0; fi
    cp -f "\$BASE/chronosync" "\$PREFIX/chronosync.tmp.\$\$" || return 1
    chmod 755 "\$PREFIX/chronosync.tmp.\$\$"
    mv -f "\$PREFIX/chronosync.tmp.\$\$" "\$PREFIX/chronosync" || return 1
    cp -f "\$BASE/run.sh" "\$PREFIX/run.sh" || return 1
    chmod 755 "\$PREFIX/run.sh"
    if [ -f "\$BASE/auth" ]; then
        cp -f "\$BASE/auth" "\$PREFIX/auth" || return 1
        chmod 600 "\$PREFIX/auth"
    else
        rm -f "\$PREFIX/auth"
    fi
    if [ -f "\$BASE/install.conf" ]; then
        cp -f "\$BASE/install.conf" "\$PREFIX/install.conf" || return 1
        chmod 600 "\$PREFIX/install.conf"
    fi
    ( cd "\$PREFIX" && nohup "\$PREFIX/run.sh" >>"\$LOG_DIR/service.log" 2>&1 & echo \$! > "\$PIDFILE" )
    sleep 0.2 2>/dev/null || sleep 1
    if is_running; then
        echo "ChronoSync started (pid \$(cat "\$PIDFILE"))."
        return 0
    fi
    echo "ChronoSync failed to start; see \$LOG_DIR/service.log" >&2
    return 1
}

stop() {
    if ! is_running; then
        rm -f "\$PIDFILE"
        echo "ChronoSync is not running."
        return 0
    fi
    pid=\$(cat "\$PIDFILE")
    kill "\$pid" 2>/dev/null
    n=0
    while kill -0 "\$pid" 2>/dev/null && [ "\$n" -lt 20 ]; do
        sleep 0.2 2>/dev/null || sleep 1
        n=\$((n + 1))
    done
    if kill -0 "\$pid" 2>/dev/null; then
        kill -9 "\$pid" 2>/dev/null
    fi
    rm -f "\$PIDFILE"
    echo "ChronoSync stopped."
}

is_running() {
    [ -f "\$PIDFILE" ] || return 1
    pid=\$(cat "\$PIDFILE" 2>/dev/null)
    [ -n "\$pid" ] || return 1
    kill -0 "\$pid" 2>/dev/null
}

status() {
    if is_running; then
        echo "ChronoSync is running (pid \$(cat "\$PIDFILE"))."
        echo "Logs: \$LOG_DIR"
        return 0
    fi
    echo "ChronoSync is not running."
    return 1
}

case "\$1" in
    start)   start ;;
    stop)    stop ;;
    restart) stop; start ;;
    status)  status ;;
    logs)    exec tail -n 100 "\$LOG_DIR/service.log" ;;
    *)       echo "Usage: \$0 {start|stop|restart|status|logs}"; exit 1 ;;
esac
EOF
    chmod 755 "$dest"
}

# ---- AudioSys CONFIG hook -------------------------------------------------
CFG_MNT=""
CFG_OWN_MOUNT=0
HOOK_BEGIN='# >>> chronosync (added by install.sh) >>>'
HOOK_END='# <<< chronosync <<<'

find_cfg() {
    CFG_MNT=""
    CFG_OWN_MOUNT=0
    for m in /mnt/cfg /mnt/config /mnt/CONFIG /media/cfg; do
        if [ -f "$m/networkfs.sh" ]; then
            CFG_MNT="$m"
            return 0
        fi
    done
    dev=""
    if [ -r /tmp/config_drv ]; then
        dev=$(cat /tmp/config_drv 2>/dev/null)
    fi
    if [ -z "$dev" ]; then
        dev=$(blkid 2>/dev/null | sed -n 's/^\([^:]*\):.*LABEL="CONFIG".*/\1/p' | head -1)
    fi
    [ -n "$dev" ] || return 1
    mount_point="/mnt/.chronosync-config.$$"
    mkdir -p "$mount_point" 2>/dev/null || return 1
    if ! mount -o rw "$dev" "$mount_point" 2>/dev/null; then
        rmdir "$mount_point" 2>/dev/null
        return 1
    fi
    if [ ! -f "$mount_point/networkfs.sh" ]; then
        umount "$mount_point" 2>/dev/null
        rmdir "$mount_point" 2>/dev/null
        return 1
    fi
    CFG_MNT="$mount_point"
    CFG_OWN_MOUNT=1
    return 0
}

release_cfg() {
    if [ "$CFG_OWN_MOUNT" = "1" ] && [ -n "$CFG_MNT" ]; then
        sync 2>/dev/null || true
        umount "$CFG_MNT" 2>/dev/null
        rmdir "$CFG_MNT" 2>/dev/null
    fi
    CFG_MNT=""
    CFG_OWN_MOUNT=0
}

strip_hook() {
    # Pure POSIX implementation; works on BusyBox without awk.
    skip=0
    while IFS= read -r line || [ -n "$line" ]; do
        case "$line" in
            "$HOOK_BEGIN"*) skip=1; continue ;;
            "$HOOK_END"*)   skip=0; continue ;;
        esac
        [ "$skip" = "1" ] || printf '%s\n' "$line"
    done < "$1"
}

append_hook() {
    cat >> "$1" <<'HOOK'

# >>> chronosync (added by install.sh) >>>
# AudioSys: /opt is RAM-backed and re-created at boot. chronosyncctl restores
# the binary from the persistent /audiosys/chronosync directory and starts it.
# Uninstall: sh /audiosys/chronosync/install.sh uninstall
if [ -x /audiosys/chronosync/chronosyncctl ]; then
    mkdir -p /usr/local/bin 2>/dev/null
    ln -sf /audiosys/chronosync/chronosyncctl /usr/local/bin/chronosyncctl 2>/dev/null
    ( /audiosys/chronosync/chronosyncctl start >/dev/null 2>&1 & )
fi
# <<< chronosync <<<
HOOK
}

install_audiosys_hook() {
    if ! find_cfg; then
        warn "没有找到 CONFIG 分区（LABEL=CONFIG），AudioSys 开机钩子未写入；重启后 /opt 会被清空，需重跑 install.sh"
        return 1
    fi
    hook_file="$CFG_MNT/networkfs.sh"
    backup="$hook_file.bak-chronosync-$(date +%Y%m%d-%H%M%S)"
    cp -a "$hook_file" "$backup" 2>/dev/null || warn "无法备份 $hook_file"
    tmp="$CFG_MNT/.networkfs.sh.chronosync.$$"
    strip_hook "$hook_file" > "$tmp" || { rm -f "$tmp"; release_cfg; die "读取 networkfs.sh 失败"; }
    append_hook "$tmp"
    if ! sh -n "$tmp" 2>/dev/null; then
        rm -f "$tmp"
        release_cfg
        die "生成的 networkfs.sh 语法检查失败，已放弃修改（原文件未覆盖）"
    fi
    cat "$tmp" > "$hook_file" || { rm -f "$tmp"; release_cfg; die "写入 networkfs.sh 失败"; }
    rm -f "$tmp"
    sync 2>/dev/null || true
    msg "已写入 AudioSys 开机钩子：$hook_file"
    release_cfg
    return 0
}

remove_audiosys_hook() {
    if ! find_cfg; then
        warn "未找到 CONFIG 分区，AudioSys 开机钩子可能仍残留"
        return 1
    fi
    hook_file="$CFG_MNT/networkfs.sh"
    if ! grep -qF "$HOOK_BEGIN" "$hook_file" 2>/dev/null; then
        release_cfg
        msg "CONFIG 分区没有 ChronoSync 钩子"
        return 0
    fi
    cp -a "$hook_file" "$hook_file.bak-chronosync-$(date +%Y%m%d-%H%M%S)" 2>/dev/null || true
    tmp="$CFG_MNT/.networkfs.sh.chronosync.$$"
    strip_hook "$hook_file" > "$tmp" || { rm -f "$tmp"; release_cfg; die "读取 networkfs.sh 失败"; }
    if ! sh -n "$tmp" 2>/dev/null; then
        rm -f "$tmp"
        release_cfg
        die "剥离钩子后的 networkfs.sh 语法检查失败，原文件未覆盖"
    fi
    cat "$tmp" > "$hook_file"
    rm -f "$tmp"
    sync 2>/dev/null || true
    msg "已从 CONFIG 分区移除 AudioSys 开机钩子"
    release_cfg
    return 0
}

# ---- normal init systems --------------------------------------------------
install_systemd() {
    unit="/etc/systemd/system/chronosync.service"
    cat > "$unit" <<EOF
[Unit]
Description=ChronoSync multi-host time calibration console
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=root
WorkingDirectory=$PREFIX
ExecStart=$PREFIX/run.sh
Restart=on-failure
RestartSec=3

[Install]
WantedBy=multi-user.target
EOF
    chmod 644 "$unit"
    systemctl daemon-reload || warn "systemctl daemon-reload 失败"
    systemctl enable chronosync.service >/dev/null 2>&1 || warn "systemd 服务 enable 失败"
    if [ "$NO_START" = "0" ]; then
        systemctl restart chronosync.service || warn "systemd 服务启动失败，请查看 journalctl -u chronosync"
    fi
}

install_openrc() {
    service="/etc/init.d/chronosync"
    cat > "$service" <<EOF
#!/sbin/openrc-run
name="chronosync"
description="ChronoSync multi-host time calibration console"
command="$PREFIX/run.sh"
command_background="yes"
pidfile="/run/chronosync.pid"

depend() {
    need net
    after firewall
}

start_pre() {
    checkpath --directory --mode 0700 "$LOG_DIR" 2>/dev/null || true
}
EOF
    chmod 755 "$service"
    rc-update add chronosync default >/dev/null 2>&1 || warn "rc-update add chronosync 失败"
    if [ "$NO_START" = "0" ]; then
        if rc-service chronosync status >/dev/null 2>&1; then
            rc-service chronosync restart || warn "OpenRC 服务重启失败"
        else
            rc-service chronosync start || warn "OpenRC 服务启动失败"
        fi
    fi
}

install_procd() {
    service="/etc/init.d/chronosync"
    cat > "$service" <<EOF
#!/bin/sh /etc/rc.common
START=99
USE_PROCD=1

start_service() {
    procd_open_instance
    procd_set_param command $PREFIX/run.sh
    procd_set_param respawn
    procd_set_param stdout 1
    procd_set_param stderr 1
    procd_close_instance
}
EOF
    chmod 755 "$service"
    "$service" enable >/dev/null 2>&1 || warn "procd 服务 enable 失败"
    if [ "$NO_START" = "0" ]; then
        "$service" restart >/dev/null 2>&1 || "$service" start || warn "procd 服务启动失败"
    fi
}

install_sysv() {
    service="/etc/init.d/S99chronosync"
    cat > "$service" <<EOF
#!/bin/sh
PREFIX="$PREFIX"
LOG_DIR="$LOG_DIR"
PIDFILE="/run/chronosync.pid"

is_running() {
    [ -f "\$PIDFILE" ] || return 1
    pid=\$(cat "\$PIDFILE" 2>/dev/null)
    [ -n "\$pid" ] && kill -0 "\$pid" 2>/dev/null
}

case "\$1" in
    start)
        [ -x "\$PREFIX/run.sh" ] || exit 1
        mkdir -p "\$LOG_DIR"
        is_running && exit 0
        ( cd "\$PREFIX" && nohup "\$PREFIX/run.sh" >>"\$LOG_DIR/service.log" 2>&1 & echo \$! > "\$PIDFILE" )
        ;;
    stop)
        if is_running; then kill "\$(cat "\$PIDFILE")" 2>/dev/null; fi
        rm -f "\$PIDFILE"
        ;;
    restart)
        "\$0" stop
        "\$0" start
        ;;
    status)
        is_running && echo "running" || echo "stopped"
        ;;
    *)
        echo "Usage: \$0 {start|stop|restart|status}"
        exit 1
        ;;
esac
EOF
    chmod 755 "$service"
    if [ "$NO_START" = "0" ]; then
        "$service" restart || warn "SysV/BusyBox 服务启动失败"
    fi
}

print_access_hint() {
    case "$BIND_HOST" in
        127.0.0.1|::1|localhost)
            msg "浏览地址 : http://127.0.0.1:$PORT（默认仅本机；远程请用 SSH 隧道）"
            ;;
        *)
            first_ip=""
            all_ips=$(hostname -I 2>/dev/null)
            if [ -n "$all_ips" ]; then
                set -- $all_ips
                first_ip="${1:-}"
            fi
            if [ -n "$first_ip" ]; then
                msg "浏览地址 : http://$first_ip:$PORT（监听 $BIND_HOST）"
            else
                msg "浏览地址 : http://$BIND_HOST:$PORT"
            fi
            if [ -z "$AUTH" ]; then
                warn "控制台监听在非回环地址且未启用访问认证；建议设置 --auth"
            fi
            warn "HTTP 未加密，请只在可信内网使用，或改用 SSH 端口转发"
            ;;
    esac
}

# ---- install / uninstall --------------------------------------------------
do_install() {
    [ -x "$BINARY" ] || die "找不到可执行文件：$BINARY（源码构建请先 cargo build --release）"
    "$BINARY" --version >/dev/null 2>&1 || die "二进制无法在当前架构运行：$BINARY"

    msg "系统类型 : $(platform_label)"
    msg "安装位置 : $PREFIX"
    msg "日志目录 : $LOG_DIR"
    msg "集群模式 : $CLUSTER_MODE"

    case "$PLATFORM" in
        audiosys)
            mkdir -p "$AUDIO_BASE" "$AUDIO_BASE/logs" || die "无法创建 $AUDIO_BASE"
            chmod 700 "$AUDIO_BASE" "$AUDIO_BASE/logs"
            install_files
            install_binary_atomic "$BINARY" "$AUDIO_BASE/chronosync" || die "复制持久二进制失败"
            self_install_copy "$AUDIO_BASE"
            write_run_script "$AUDIO_BASE/run.sh" "$AUDIO_BASE"
            write_auth "$AUDIO_BASE/auth"
            write_install_conf "$AUDIO_BASE"
            install_cluster_files "$AUDIO_BASE" "$PREFIX/cluster.key"
            write_chronosyncctl "$AUDIO_BASE/chronosyncctl"
            mkdir -p /usr/local/bin
            ln -sf "$AUDIO_BASE/chronosyncctl" /usr/local/bin/chronosyncctl
            install_audiosys_hook || true
            if [ "$NO_START" = "0" ]; then
                "$AUDIO_BASE/chronosyncctl" restart || warn "ChronoSync 启动失败，请查看 $LOG_DIR/service.log"
            fi
            msg "AudioSys 持久目录 : $AUDIO_BASE"
            msg "控制命令 : chronosyncctl {start|stop|restart|status|logs}"
            ;;
        systemd)
            install_files
            install_systemd
            ;;
        openrc)
            install_files
            install_openrc
            ;;
        procd)
            install_files
            install_procd
            ;;
        sysv)
            install_files
            install_sysv
            warn "当前是 BusyBox/SysV 风格系统；如果根文件系统位于内存盘，重启后 $PREFIX 可能被清空"
            ;;
        *)
            install_files
            warn "未识别 init 系统，未注册开机启动"
            msg "手动启动：$PREFIX/run.sh"
            ;;
    esac

    print_access_hint
    msg "安装完成。日志：$LOG_DIR"
}

do_uninstall() {
    msg "开始卸载 ChronoSync（系统类型：$(platform_label)）"
    case "$PLATFORM" in
        audiosys)
            if [ -x "$AUDIO_BASE/chronosyncctl" ]; then
                "$AUDIO_BASE/chronosyncctl" stop >/dev/null 2>&1 || true
            fi
            if is_running_pidfile /run/chronosync.pid; then
                kill "$(cat /run/chronosync.pid)" 2>/dev/null || true
            fi
            rm -f /run/chronosync.pid
            rm -f /usr/local/bin/chronosyncctl
            remove_audiosys_hook || true
            rm -rf "$PREFIX"
            if [ "$PURGE" = "1" ]; then
                rm -rf "$AUDIO_BASE"
                msg "已删除 AudioSys 持久目录和日志：$AUDIO_BASE"
            else
                rm -f "$AUDIO_BASE/chronosync" "$AUDIO_BASE/run.sh" "$AUDIO_BASE/auth" \
                      "$AUDIO_BASE/chronosyncctl" "$AUDIO_BASE/install.sh" "$AUDIO_BASE/install.conf" \
                      "$AUDIO_BASE/cluster.key"
                rm -rf "$AUDIO_BASE/cluster-state"
                msg "已保留日志：$LOG_DIR（彻底删除请加 --purge）"
            fi
            ;;
        systemd)
            systemctl disable --now chronosync.service >/dev/null 2>&1 || true
            rm -f /etc/systemd/system/chronosync.service
            systemctl daemon-reload >/dev/null 2>&1 || true
            rm -rf "$PREFIX"
            ;;
        openrc)
            rc-service chronosync stop >/dev/null 2>&1 || true
            rc-update del chronosync default >/dev/null 2>&1 || true
            rm -f /etc/init.d/chronosync
            rm -rf "$PREFIX"
            ;;
        procd)
            /etc/init.d/chronosync stop >/dev/null 2>&1 || true
            /etc/init.d/chronosync disable >/dev/null 2>&1 || true
            rm -f /etc/init.d/chronosync
            rm -rf "$PREFIX"
            ;;
        sysv)
            [ -x /etc/init.d/S99chronosync ] && /etc/init.d/S99chronosync stop >/dev/null 2>&1 || true
            rm -f /etc/init.d/S99chronosync
            rm -rf "$PREFIX"
            ;;
        *)
            rm -rf "$PREFIX"
            ;;
    esac
    if [ "$PURGE" = "1" ] && [ "$PLATFORM" != "audiosys" ]; then
        rm -rf "$LOG_DIR"
        msg "已删除日志目录：$LOG_DIR"
    fi
    msg "卸载完成。"
}

do_status() {
    msg "ChronoSync 状态"
    if [ -f "$PREFIX/install.conf" ]; then
        # shellcheck disable=SC1091
        . "$PREFIX/install.conf"
        msg "已安装版本 : $VERSION"
        msg "系统类型   : $PLATFORM"
        msg "监听地址   : $BIND:$PORT"
        msg "日志目录   : $LOG_DIR"
    else
        msg "当前未发现 /opt/chronosync 安装记录"
    fi
    if [ -x "$PREFIX/chronosync" ]; then
        msg "程序文件   : $PREFIX/chronosync"
    else
        msg "程序文件   : 缺失"
    fi
    case "$PLATFORM" in
        audiosys)
            if [ -x "$AUDIO_BASE/chronosyncctl" ]; then
                "$AUDIO_BASE/chronosyncctl" status || true
            fi
            if [ -f "$AUDIO_BASE/chronosyncctl" ]; then
                msg "AudioSys钩子: $(grep -qF "$HOOK_BEGIN" /tmp/networkfs.sh 2>/dev/null && echo '当前启动副本已加载' || echo '持久钩子需在 CONFIG 分区确认')"
            fi
            ;;
        systemd)
            systemctl is-active chronosync.service 2>/dev/null || true
            ;;
        openrc)
            rc-service chronosync status 2>/dev/null || true
            ;;
        procd|sysv)
            [ -x /etc/init.d/chronosync ] && /etc/init.d/chronosync status 2>/dev/null || true
            [ -x /etc/init.d/S99chronosync ] && /etc/init.d/S99chronosync status 2>/dev/null || true
            ;;
    esac
}

case "$ACTION" in
    install)   do_install ;;
    uninstall) do_uninstall ;;
    status)    do_status ;;
esac
exit 0
