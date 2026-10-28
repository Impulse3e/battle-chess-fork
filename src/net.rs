//! Мультиплеер (с 0.45, просьба пользователя): игра подключается к ретранслятору
//! `tools/mp_server.py` по TCP и обменивается с соперником строками. Партия детерминирована
//! (карт и усиленного компьютера нет, баланс — стандартный), поэтому достаточно передавать
//! действия: ход фигурой или выставление из запаса (`M …`) и кнопку «Конец хода» (`E`). Обе игры
//! делают одни и те же действия и показывают одну позицию. Шифрования нет.

use std::io::{BufRead, BufReader, Write};
use std::net::{Shutdown, TcpStream, ToSocketAddrs};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use eframe::egui;

use crate::game::{Color, Kind, Move, MoveKind};

/// Порт сервера, если в адресе его нет.
pub const DEFAULT_PORT: u16 = 47900;

/// Пульс (с 0.49): игра шлёт серверу `PING` раз в `PING_EVERY`, сервер — игре раз в 20 с.
/// Без него роутеры забывают «молчащее» соединение, пока соперник думает, и ход не доходит, а
/// обе стороны считают соединение живым. Нет ничего от сервера `SILENCE` — связь потеряна.
pub const PING: &str = "PING";
const PING_EVERY: Duration = Duration::from_secs(15);
const SILENCE: Duration = Duration::from_secs(60);

/// Что пришло от сервера или соперника.
#[derive(Debug, PartialEq, Eq)]
pub enum NetEvent {
    /// Соединение установлено.
    Connected,
    /// Ждём второго игрока.
    Waiting,
    /// Партия началась: наш цвет.
    Start(Color),
    /// Строка от соперника (ход, конец хода, версия).
    Line(String),
    /// Соперник отключился.
    Left,
    /// Соединение закрыто или не открылось: причина по-русски.
    Closed(String),
}

/// Соединение с сервером: события читает фоновый поток, пишем — сюда.
pub struct Link {
    pub rx: Receiver<NetEvent>,
    writer: Arc<Mutex<Option<TcpStream>>>,
}

impl Link {
    /// Подключается к `address` (`IP:порт` или `IP` — тогда порт `DEFAULT_PORT`) в фоновом
    /// потоке; о результате и обо всём, что придёт потом, скажут события `rx`.
    pub fn connect(address: &str, ctx: egui::Context) -> Link {
        let (tx, rx) = mpsc::channel();
        let writer = Arc::new(Mutex::new(None));
        let shared = Arc::clone(&writer);
        let address = address.trim().to_owned();
        let _ = std::thread::Builder::new()
            .name("multiplayer".into())
            .spawn(move || run(&address, &tx, &shared, &ctx));
        let pulse = Arc::clone(&writer);
        let _ = std::thread::Builder::new()
            .name("multiplayer-ping".into())
            .spawn(move || heartbeat(pulse));
        Link { rx, writer }
    }

    /// Шлёт строку сопернику через сервер; ошибку заметит читающий поток.
    pub fn send(&self, line: &str) {
        if let Some(stream) = self
            .writer
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_mut()
        {
            let _ = stream.write_all(format!("{line}\n").as_bytes());
        }
    }
}

impl Drop for Link {
    /// Закрываем соединение: сервер скажет сопернику, что мы ушли, поток чтения завершится.
    fn drop(&mut self) {
        if let Some(stream) = self.writer.lock().unwrap_or_else(|e| e.into_inner()).take() {
            let _ = stream.shutdown(Shutdown::Both);
        }
    }
}

fn run(
    address: &str,
    tx: &Sender<NetEvent>,
    writer: &Mutex<Option<TcpStream>>,
    ctx: &egui::Context,
) {
    let event = |e: NetEvent| {
        let _ = tx.send(e);
        ctx.request_repaint();
    };
    let stream = match open(address) {
        Ok(stream) => stream,
        Err(reason) => return event(NetEvent::Closed(reason)),
    };
    let reader = match stream.try_clone() {
        Ok(reader) => reader,
        Err(e) => return event(NetEvent::Closed(format!("ошибка соединения: {e}"))),
    };
    let _ = reader.set_read_timeout(Some(SILENCE));
    *writer.lock().unwrap_or_else(|e| e.into_inner()) = Some(stream);
    event(NetEvent::Connected);
    let reason = read_lines(reader, &event);
    event(NetEvent::Closed(reason));
}

/// Читает строки сервера, пока соединение живо; возвращает, почему оно кончилось. Пульс
/// сервера (`PING`) не передаётся дальше — он только продлевает ожидание.
fn read_lines(reader: TcpStream, event: &impl Fn(NetEvent)) -> String {
    let mut reader = BufReader::new(reader);
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => return "соединение с сервером закрыто".into(),
            Ok(_) => {}
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                return "сервер не отвечает больше минуты".into();
            }
            Err(e) => return format!("ошибка соединения: {e}"),
        }
        let line = line.trim();
        event(match line {
            PING => continue,
            "WAIT" => NetEvent::Waiting,
            "START white" => NetEvent::Start(Color::White),
            "START black" => NetEvent::Start(Color::Black),
            "LEFT" => NetEvent::Left,
            _ => NetEvent::Line(line.to_owned()),
        });
    }
}

/// Шлёт серверу `PING`, пока соединение открыто (`Link::drop` забирает поток записи).
fn heartbeat(writer: Arc<Mutex<Option<TcpStream>>>) {
    loop {
        std::thread::sleep(PING_EVERY);
        let mut guard = writer.lock().unwrap_or_else(|e| e.into_inner());
        let Some(stream) = guard.as_mut() else {
            return;
        };
        if stream.write_all(format!("{PING}\n").as_bytes()).is_err() {
            return;
        }
    }
}

/// Адрес с портом. Без порта — `DEFAULT_PORT`. IPv6 (`::1`, `[::1]`) тоже: двоеточие в нём
/// само по себе порт не означает. `[::1]:47900` и `127.0.0.1:47900` не меняются.
fn with_default_port(address: &str) -> String {
    if let Some(rest) = address.strip_prefix('[') {
        if address.contains("]:") {
            return address.to_owned();
        }
        let host = rest.strip_suffix(']').unwrap_or(rest);
        return format!("[{host}]:{DEFAULT_PORT}");
    }
    match address.matches(':').count() {
        0 => format!("{address}:{DEFAULT_PORT}"),
        1 => address.to_owned(),
        _ => format!("[{address}]:{DEFAULT_PORT}"),
    }
}

fn open(address: &str) -> Result<TcpStream, String> {
    if address.is_empty() {
        return Err("введите адрес сервера".into());
    }
    let full = with_default_port(address);
    let addrs: Vec<_> = full
        .to_socket_addrs()
        .map_err(|e| format!("адрес «{address}» не разобрать: {e}"))?
        .collect();
    let mut last = String::from("адрес не найден");
    for addr in addrs {
        match TcpStream::connect_timeout(&addr, Duration::from_secs(5)) {
            Ok(stream) => {
                let _ = stream.set_nodelay(true);
                return Ok(stream);
            }
            Err(e) => last = format!("не удалось подключиться к {addr}: {e}"),
        }
    }
    Err(last)
}

// ---------- Ходы строкой ----------

/// Конец хода — кнопка «Конец хода».
pub const END_TURN: &str = "E";

/// Правила партии от белых (кто зашёл первым): перемещение при взятии.
pub fn rules_line(advance: bool) -> String {
    format!("RULES advance={}", u8::from(advance))
}

/// Правила из строки `rules_line`.
pub fn parse_rules(line: &str) -> Option<bool> {
    match line.strip_prefix("RULES ")?.trim() {
        "advance=1" => Some(true),
        "advance=0" => Some(false),
        _ => None,
    }
}

/// Версия игры: разные версии могут считать партию по-разному.
pub fn version_line() -> String {
    format!("VER {}", env!("CARGO_PKG_VERSION"))
}

/// «M откуда куда вид превращение»: клетки — номера 0…63; вид — 0 обычный ход, 1 и 2 —
/// короткая и длинная рокировка, 10 + `Kind` — выставление из запаса; превращение — 0 или
/// 1 + `Kind`.
pub fn encode_move(mv: Move) -> String {
    let kind: u16 = match mv.kind {
        MoveKind::Quiet => 0,
        MoveKind::CastleShort => 1,
        MoveKind::CastleLong => 2,
        MoveKind::Drop(k) => 10 + k as u16,
        MoveKind::FireDrop(k) => 20 + k as u16,
        MoveKind::SparkDrop(k) => 30 + k as u16,
        MoveKind::WindDrop(k) => 40 + k as u16,
        MoveKind::GoldDrop(k) => 50 + k as u16,
        MoveKind::ShadeDrop(k) => 60 + k as u16,
        MoveKind::IceDrop(k) => 70 + k as u16,
        MoveKind::BarbarianDrop(k) => 80 + k as u16,
        MoveKind::WizardDrop(k) => 90 + k as u16,
        MoveKind::ComboDrop(k, i) => 100 + u16::from(i) * 6 + k as u16,
    };
    let promotion = mv.promotion.map_or(0, |k| 1 + k as u8);
    format!("M {} {} {kind} {promotion}", mv.from, mv.to)
}

/// Ход из строки `encode_move` — только если он есть среди законных `legal`.
pub fn decode_move(line: &str, legal: &[Move]) -> Option<Move> {
    let mut parts = line.strip_prefix("M ")?.split_whitespace();
    // Код хода — до u16: объединённых фигур в запасе много.
    let mut number = || parts.next()?.parse::<u16>().ok();
    let (from, to, kind, promotion) = (number()?, number()?, number()?, number()?);
    let (from, to) = (u8::try_from(from).ok()?, u8::try_from(to).ok()?);
    let kind_of = |n: u16| Kind::ALL.into_iter().find(|&k| k as u16 == n);
    let kind = match kind {
        0 => MoveKind::Quiet,
        1 => MoveKind::CastleShort,
        2 => MoveKind::CastleLong,
        n @ 100.. => {
            MoveKind::ComboDrop(kind_of((n - 100) % 6)?, u8::try_from((n - 100) / 6).ok()?)
        }
        n @ 90.. => MoveKind::WizardDrop(kind_of(n - 90)?),
        n @ 80.. => MoveKind::BarbarianDrop(kind_of(n - 80)?),
        n @ 70.. => MoveKind::IceDrop(kind_of(n - 70)?),
        n @ 60.. => MoveKind::ShadeDrop(kind_of(n - 60)?),
        n @ 50.. => MoveKind::GoldDrop(kind_of(n - 50)?),
        n @ 40.. => MoveKind::WindDrop(kind_of(n - 40)?),
        n @ 30.. => MoveKind::SparkDrop(kind_of(n - 30)?),
        n @ 20.. => MoveKind::FireDrop(kind_of(n - 20)?),
        n => MoveKind::Drop(kind_of(n.checked_sub(10)?)?),
    };
    let promotion = match promotion {
        0 => None,
        n => Some(kind_of(n - 1)?),
    };
    let mv = Move {
        from,
        to,
        kind,
        promotion,
    };
    legal.contains(&mv).then_some(mv)
}

// ---------- Последний адрес ----------

fn address_path() -> Option<PathBuf> {
    crate::settings::data_dir().map(|dir| dir.join("server.txt"))
}

/// Адрес сервера, к которому подключались в прошлый раз.
pub fn saved_address() -> String {
    address_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|s| s.trim().to_owned())
        .unwrap_or_default()
}

pub fn save_address(address: &str) {
    if let Some(path) = address_path() {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(path, address.trim());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::Position;

    #[test]
    fn every_legal_move_survives_the_wire() {
        let pos = Position::start();
        let legal = pos.legal_moves();
        for &mv in &legal {
            assert_eq!(decode_move(&encode_move(mv), &legal), Some(mv));
        }
        assert_eq!(decode_move("M 12 40 0 0", &legal), None, "незаконный ход");
        assert_eq!(decode_move("мусор", &legal), None);
        assert_eq!(decode_move("M 1 2", &legal), None);
        for advance in [false, true] {
            assert_eq!(parse_rules(&rules_line(advance)), Some(advance));
        }
        assert_eq!(parse_rules("RULES что-то"), None);
    }

    #[test]
    fn an_address_without_a_port_gets_the_default_including_ipv6() {
        assert_eq!(with_default_port("127.0.0.1"), "127.0.0.1:47900");
        assert_eq!(with_default_port("127.0.0.1:14888"), "127.0.0.1:14888");
        assert_eq!(with_default_port("localhost"), "localhost:47900");
        assert_eq!(with_default_port("::1"), "[::1]:47900");
        assert_eq!(with_default_port("[::1]"), "[::1]:47900");
        assert_eq!(with_default_port("[::1]:47900"), "[::1]:47900");
    }

    /// Две игры через настоящий сервер играют одну партию случайными ходами: после каждого
    /// действия позиции совпадают. Нужен запущенный `python3 tools/mp_server.py 47911`.
    #[test]
    #[ignore]
    fn two_games_stay_in_sync_through_the_server() {
        use crate::game::Game;
        use std::time::Instant;
        let ctx = egui::Context::default();
        let links = [
            Link::connect("127.0.0.1:47911", ctx.clone()),
            Link::connect("127.0.0.1:47911", ctx),
        ];
        let next = |link: &Link| loop {
            match link
                .rx
                .recv_timeout(Duration::from_secs(5))
                .expect("сервер молчит")
            {
                NetEvent::Connected | NetEvent::Waiting => {}
                other => return other,
            }
        };
        let mut colors = Vec::new();
        for link in &links {
            match next(link) {
                NetEvent::Start(color) => colors.push(color),
                other => panic!("ждали START, пришло {other:?}"),
            }
        }
        assert_ne!(colors[0], colors[1]);
        let fresh = || Game::from_position(Position::start().without_cards().with_advance(true));
        let mut games = [fresh(), fresh()];
        let mut seed = 0x2545_F491_4F6C_DD1Du64;
        let started = Instant::now();
        let mut actions = 0;
        while !games[0].is_over() && actions < 400 {
            let side = games[0].position().side();
            let me = colors.iter().position(|&c| c == side).unwrap();
            let them = 1 - me;
            // Ходит тот, чья очередь, — шлёт ход; соперник принимает и делает то же.
            let line = {
                let legal = games[me].legal_moves();
                if legal.is_empty() {
                    assert!(games[me].end_turn());
                    END_TURN.to_owned()
                } else {
                    seed ^= seed << 13;
                    seed ^= seed >> 7;
                    seed ^= seed << 17;
                    let mv = legal[(seed % legal.len() as u64) as usize];
                    games[me].play(mv).unwrap();
                    encode_move(mv)
                }
            };
            links[me].send(&line);
            let NetEvent::Line(got) = next(&links[them]) else {
                panic!("ждали ход");
            };
            assert_eq!(got, line);
            if got == END_TURN {
                assert!(games[them].end_turn());
            } else {
                let mv = decode_move(&got, games[them].legal_moves()).expect("ход законен");
                games[them].play(mv).unwrap();
            }
            assert_eq!(games[0].position(), games[1].position(), "после «{line}»");
            actions += 1;
        }
        eprintln!(
            "{actions} действий за {:?}, итог: {:?}",
            started.elapsed(),
            games[0].status()
        );
        // Ушёл один — второй узнаёт.
        let [first, second] = links;
        drop(first);
        assert_eq!(next(&second), NetEvent::Left);
    }
}
