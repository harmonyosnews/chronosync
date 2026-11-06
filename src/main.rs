mod app;
mod cluster;
mod local;
mod ntp;
mod remote;
mod scan;

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::time::Duration;

use base64::Engine;
use clap::Parser;

#[derive(Parser, Debug)]
#[command(
    name = "chronosync",
    version,
    about = "ChronoSync — 通过 SSH 批量校准 Linux 设备时间的中文 Web 控制台"
)]
struct Cli {
    /// Web 控制台监听地址（默认 0.0.0.0，局域网可访问；建议配合 --auth 或使用可信内网）
    #[arg(long, default_value = "0.0.0.0")]
    bind: String,

    /// Web 控制台端口
    #[arg(long, default_value_t = 8686)]
    port: u16,

    /// NTP/SNTP 时间源，多个用逗号分隔
    #[arg(
        long,
        value_delimiter = ',',
        default_value = "ntp.aliyun.com,ntp1.aliyun.com,cn.pool.ntp.org,time.cloudflare.com"
    )]
    ntp_servers: Vec<String>,

    /// 可选的 HTTP Basic 认证，格式 user:password（也可用环境变量 CHRONOSYNC_AUTH）
    #[arg(long, env = "CHRONOSYNC_AUTH", hide_env_values = true)]
    auth: Option<String>,

    /// 运行日志保存目录
    #[arg(long, default_value = "./logs")]
    log_dir: PathBuf,

    /// 允许在全部 NTP 源不可达时降级使用本机系统时间（默认拒绝，以免写入错误时间）
    #[arg(long)]
    allow_system_clock: bool,

    /// 启动并成功监听后，自动用默认浏览器打开 WebUI
    #[arg(long)]
    open: bool,

    /// 请求端口被占用时，自动尝试后续 10 个端口
    #[arg(long)]
    auto_port: bool,

    /// 集群模式：off / server / client / auto
    #[arg(
        long,
        default_value = "off",
        value_parser = ["off", "server", "client", "auto"]
    )]
    cluster_mode: String,

    /// 集群标识，同一集群必须一致
    #[arg(long, default_value = "chronosync")]
    cluster_id: String,

    /// 固定节点标识；省略时在状态目录生成并复用
    #[arg(long)]
    node_id: Option<String>,

    /// 集群共享密钥文件（非 off 模式必填，至少 16 字节）
    #[arg(long)]
    cluster_secret_file: Option<PathBuf>,

    /// 集群 UDP 绑定地址（测试多实例时可指定回环地址）
    #[arg(long, default_value = "0.0.0.0")]
    cluster_bind: String,

    /// 集群发现 UDP 端口
    #[arg(long, default_value_t = 18686)]
    discovery_port: u16,

    /// 集群时间服务 UDP 端口
    #[arg(long, default_value_t = 18687)]
    time_port: u16,

    /// 是否允许本节点被选为 server
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
    server_eligible: bool,

    /// client 自动写入本机时间；默认关闭，只检测
    #[arg(long)]
    auto_apply: bool,

    /// 自动写入的最小偏差阈值（毫秒）
    #[arg(long, default_value_t = 100)]
    max_offset_ms: i64,

    /// 单次允许的最大时间跳变（毫秒）
    #[arg(long, default_value_t = 5000)]
    max_step_ms: i64,

    /// 允许超过 max_step_ms 的大步长写入
    #[arg(long)]
    allow_large_step: bool,

    /// client 同步间隔（秒）
    #[arg(long, default_value_t = 60)]
    sync_interval: u64,

    /// 集群状态目录（node-id）
    #[arg(long, default_value = "./cluster-state")]
    cluster_state_dir: PathBuf,

    /// 额外 seed 地址（ip:port），可重复或逗号分隔
    #[arg(long, value_delimiter = ',')]
    cluster_seed: Vec<String>,
}

struct ExistingInstance {
    version: Option<String>,
    auth_required: bool,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    let auth_header = match cli.auth.as_deref() {
        Some(value) => {
            if !value.contains(':') {
                eprintln!("错误：--auth 必须使用 user:password 格式");
                std::process::exit(2);
            }
            let encoded = base64::engine::general_purpose::STANDARD.encode(value.as_bytes());
            Some(format!("Basic {encoded}"))
        }
        None => None,
    };

    if cli.ntp_servers.is_empty() {
        eprintln!("错误：至少需要提供一个 --ntp-servers 时间源");
        std::process::exit(2);
    }

    let auth_enabled = auth_header.is_some();
    let local_environment = local::environment();
    let config = app::Config {
        ntp_servers: cli.ntp_servers.clone(),
        allow_system_clock: cli.allow_system_clock,
        log_dir: cli.log_dir.clone(),
        auth_header,
    };
    let app = app::App::new(config, local_environment).await;
    let cluster_mode = cluster::Mode::parse(&cli.cluster_mode).unwrap_or_else(|| {
        eprintln!("错误：无效的 --cluster-mode {}", cli.cluster_mode);
        std::process::exit(2);
    });
    if cluster_mode != cluster::Mode::Off {
        let secret_path = cli.cluster_secret_file.as_ref().unwrap_or_else(|| {
            eprintln!("错误：集群模式需要 --cluster-secret-file");
            std::process::exit(2);
        });
        let secret = cluster::load_secret(secret_path).unwrap_or_else(|err| {
            eprintln!("错误：{err}");
            std::process::exit(2);
        });
        let node_id = cluster::resolve_node_id(cli.node_id.as_deref(), &cli.cluster_state_dir)
            .unwrap_or_else(|err| {
                eprintln!("错误：{err}");
                std::process::exit(2);
            });
        let listen_ip = cli.cluster_bind.parse().unwrap_or_else(|_| {
            eprintln!("错误：无效的 --cluster-bind {}", cli.cluster_bind);
            std::process::exit(2);
        });
        let cluster_cfg = cluster::Config {
            mode: cluster_mode,
            cluster_id: cli.cluster_id.clone(),
            node_id,
            secret,
            listen_ip,
            discovery_port: cli.discovery_port,
            time_port: cli.time_port,
            server_eligible: cli.server_eligible,
            auto_apply: cli.auto_apply,
            max_offset_ms: cli.max_offset_ms,
            max_step_ms: cli.max_step_ms,
            allow_large_step: cli.allow_large_step,
            sync_interval: Duration::from_secs(cli.sync_interval),
            seed_peers: cli.cluster_seed.clone(),
        };
        match cluster::start(cluster_cfg, app.clone(), app.clone()).await {
            Ok(cluster) => app.set_cluster(cluster),
            Err(err) => {
                eprintln!("错误：集群启动失败：{err}");
                std::process::exit(1);
            }
        }
    }
    if cluster_mode != cluster::Mode::Off {
        let refresh_app = app.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(60));
            loop {
                tick.tick().await;
                let _ = refresh_app.refresh_reference().await;
            }
        });
    }
    let router = app.router();

    let attempts: u16 = if cli.auto_port { 10 } else { 0 };
    let mut chosen_port = cli.port;
    let mut listener = None;
    let mut last_conflict_port = cli.port;
    let mut last_bind_error: Option<String> = None;
    for offset in 0..=attempts {
        let Some(port) = cli.port.checked_add(offset) else {
            break;
        };
        match tokio::net::TcpListener::bind((cli.bind.as_str(), port)).await {
            Ok(bound) => {
                chosen_port = port;
                listener = Some(bound);
                if offset > 0 {
                    println!(
                        "提示：端口 {} 不可用，已自动改用 {}。",
                        cli.port, chosen_port
                    );
                }
                break;
            }
            Err(err) if err.kind() == std::io::ErrorKind::AddrInUse => {
                last_conflict_port = port;
                let existing = if offset == 0 {
                    probe_existing(port)
                } else {
                    probe_once(port)
                };
                if let Some(existing) = existing {
                    announce_existing(&cli, port, existing);
                    return;
                }
                last_bind_error = Some(err.to_string());
                if !cli.auto_port {
                    break;
                }
                if offset == 0 {
                    eprintln!("提示：端口 {} 被占用，正在尝试后续端口…", port);
                }
            }
            Err(err) => {
                last_conflict_port = port;
                last_bind_error = Some(err.to_string());
                if cli.auto_port {
                    if offset == 0 {
                        eprintln!("提示：端口 {} 不可用（{}），正在尝试后续端口…", port, err);
                    }
                    continue;
                }
                eprintln!("错误：无法监听 {}:{}：{err}", cli.bind, port);
                std::process::exit(1);
            }
        }
    }
    let Some(listener) = listener else {
        report_port_conflict(last_conflict_port, cli.auto_port);
        if let Some(error) = last_bind_error {
            eprintln!("  最后错误：{error}");
        }
        std::process::exit(1);
    };

    let address = listener
        .local_addr()
        .map(|addr| addr.to_string())
        .unwrap_or_else(|_| format!("{}:{}", cli.bind, chosen_port));

    println!("\n{}", app.console_summary(&address));
    println!("\n可用访问地址：");
    for url in access_urls(&cli.bind, chosen_port) {
        println!("  {url}");
    }
    if cfg!(windows) {
        println!(
            "\nWindows 提示：如果局域网内打不开，请在 Windows 防火墙中允许 chronosync.exe 或 TCP 端口 {}。",
            chosen_port
        );
    }
    let exposed = !matches!(cli.bind.as_str(), "127.0.0.1" | "::1" | "localhost");
    if exposed {
        println!();
        println!("安全提醒：控制台正在所有网卡上监听，且未启用 TLS。");
        println!(
            "          浏览器提交的 SSH 密码会以明文经过网络；请优先使用可信内网并设置 --auth。"
        );
        if !auth_enabled {
            println!("          当前未启用访问认证；建议至少设置 --auth user:password。");
        }
    }
    println!("\n按 Ctrl+C 停止服务；关闭本控制台窗口也会停止服务。\n");

    if cli.open {
        let url = local_url(chosen_port);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(300)).await;
            open_browser(&url);
        });
    }

    if let Err(err) = axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_signal())
        .await
    {
        eprintln!("服务异常退出：{err}");
        std::process::exit(1);
    }
}

fn announce_existing(cli: &Cli, port: u16, existing: ExistingInstance) {
    println!(
        "\n检测到 ChronoSync 已在端口 {} 运行，本次不会重复启动。",
        port
    );
    if let Some(version) = existing.version {
        println!("已运行版本：{version}");
    }
    if existing.auth_required {
        println!("该实例启用了访问认证，浏览器会提示输入用户名和密码。");
    }
    println!("\n可用访问地址：");
    for url in access_urls(&cli.bind, port) {
        println!("  {url}");
    }
    if cli.open {
        open_browser(&local_url(port));
    }
    print_stop_hint();
}

fn report_port_conflict(port: u16, auto_tried: bool) {
    if auto_tried {
        eprintln!(
            "\n错误：端口 {} 及其后续 10 个端口均不可用，ChronoSync 未能启动。",
            port
        );
    } else {
        eprintln!(
            "\n错误：端口 {} 已被其他程序占用，ChronoSync 未能启动。",
            port
        );
        eprintln!("  - 自动换端口：chronosync --auto-port");
    }
    eprintln!(
        "  - 手动换端口：chronosync --port {}",
        port.saturating_add(1)
    );
    if cfg!(windows) {
        eprintln!("  - 查看占用：netstat -ano | findstr :{}", port);
        eprintln!("  - 如果结果显示的是 CLOSE_WAIT/ESTABLISHED 而不是 LISTENING，");
        eprintln!("    说明是其他程序的本地连接，不是 ChronoSync；先用");
        eprintln!("      tasklist /FI \"PID eq <PID>\"");
        eprintln!("    确认进程，再执行：taskkill /F /PID <PID>");
        eprintln!("  - Windows 没有 kill/pkill 命令，请使用 taskkill");
    } else {
        eprintln!("  - 查看占用：ss -ltnp | grep :{}", port);
        eprintln!("  - 必要时结束占用进程，或关闭占用该端口的程序");
    }
}

fn local_url(port: u16) -> String {
    format!("http://127.0.0.1:{port}/")
}

fn access_urls(bind: &str, port: u16) -> Vec<String> {
    let local = local_url(port);
    if matches!(bind, "127.0.0.1" | "::1" | "localhost") {
        return vec![local];
    }

    let mut urls = vec![local];
    if bind == "0.0.0.0" || bind == "::" || bind == "[::]" {
        if let Ok(interfaces) = if_addrs::get_if_addrs() {
            for interface in interfaces {
                let if_addrs::IfAddr::V4(v4) = interface.addr else {
                    continue;
                };
                if v4.ip.is_loopback() || v4.ip.is_unspecified() {
                    continue;
                }
                let url = format!("http://{}:{}/", v4.ip, port);
                if !urls.contains(&url) {
                    urls.push(url);
                }
            }
        }
    } else {
        let host = if bind.contains(':') && !bind.starts_with('[') {
            format!("[{bind}]")
        } else {
            bind.to_string()
        };
        let url = format!("http://{host}:{port}/");
        if !urls.contains(&url) {
            urls.push(url);
        }
    }
    urls.truncate(8);
    urls
}

fn probe_existing(port: u16) -> Option<ExistingInstance> {
    // A second launch may happen while the first process is still doing its
    // initial NTP query. Retry briefly so the already-running WebUI is found.
    for _ in 0..3 {
        if let Some(info) = probe_once(port) {
            return Some(info);
        }
        std::thread::sleep(Duration::from_millis(300));
    }
    None
}

fn probe_once(port: u16) -> Option<ExistingInstance> {
    let address = SocketAddr::from(([127, 0, 0, 1], port));
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_millis(450)).ok()?;
    stream
        .set_write_timeout(Some(Duration::from_millis(500)))
        .ok()?;
    stream
        .set_read_timeout(Some(Duration::from_millis(700)))
        .ok()?;
    let request =
        format!("GET /api/health HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n");
    stream.write_all(request.as_bytes()).ok()?;

    let mut data = Vec::new();
    let mut chunk = [0u8; 2048];
    loop {
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(count) => {
                data.extend_from_slice(&chunk[..count]);
                if data.len() > 64 * 1024 {
                    break;
                }
            }
        }
    }
    let text = String::from_utf8_lossy(&data);
    let lower = text.to_ascii_lowercase();
    let auth_required = lower.contains("basic realm=\"chronosync\"");
    let is_ours = auth_required || (text.contains("\"ok\":true") && text.contains("\"version\""));
    if !is_ours {
        return None;
    }
    Some(ExistingInstance {
        version: extract_json_string(&text, "version"),
        auth_required,
    })
}

fn extract_json_string(text: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\"");
    let start = text.find(&needle)?;
    let after_key = &text[start + needle.len()..];
    let after_colon = after_key.split_once(':')?.1.trim_start();
    let value = after_colon.strip_prefix('"')?;
    let end = value.find('"')?;
    Some(value[..end].to_string())
}

fn print_stop_hint() {
    if cfg!(windows) {
        println!("\n要停止已有实例，可关闭它的控制台窗口，或运行 stop-chronosync.bat。");
        println!("也可以执行：taskkill /F /IM chronosync.exe");
    } else {
        println!("\n要停止已有实例，请关闭它的终端窗口，或结束对应 chronosync 进程。");
    }
}

#[cfg(windows)]
fn open_browser(url: &str) {
    use windows_sys::Win32::UI::Shell::ShellExecuteW;

    let operation: Vec<u16> = "open".encode_utf16().chain(std::iter::once(0)).collect();
    let file: Vec<u16> = url.encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: both UTF-16 buffers are null-terminated and remain alive for the call.
    unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            operation.as_ptr(),
            file.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            1,
        );
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
fn open_browser(url: &str) {
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
}

#[cfg(target_os = "macos")]
fn open_browser(url: &str) {
    let _ = std::process::Command::new("open").arg(url).spawn();
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut signal) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            signal.recv().await;
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    println!("\n收到退出信号，正在关闭 ChronoSync…");
}
