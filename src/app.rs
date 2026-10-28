//! Окно игры: изометрическая доска в духе Into the Breach, анимации ударов, боковые
//! панели и компьютер.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use eframe::egui::text::{LayoutJob, TextFormat};
use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, CursorIcon, FontId, Key, Layout, Margin, Mesh,
    Painter, Pos2, Rect, RichText, Sense, Shape, Stroke, Vec2, pos2, vec2,
};

mod menu;
mod table;

use crate::assets::{
    ANCHOR, Art, BLESS_ANCHOR, BLESS_FIGURE, BOARD, CRATER_CENTER, CRATER_FRAME_TIME,
    CRATER_FRAMES, CURSOR, CURSOR_HOTSPOT, DARK_LOOP, ELECTRIC_ANCHOR, ELECTRIC_FLASH,
    ELECTRIC_FRAME_TIME, ELECTRIC_LOOP, ELECTRIC_SLOW, FREEZE_CENTER, FREEZE_FROST, GLYPH_HEIGHT,
    Glyph, LAVA_FRAME_TIME, LAVA_FRAMES, LAVA_HOLD, LAVA_IGNITE, LAVA_SPREAD, LAVA_WAVE_AT,
    METEOR_CENTER, METEOR_FRAME_TIME, METEOR_FRAMES, METEOR_IMPACT, METEOR_WAVE_FRAMES, Marker,
    POISON_LOOP, POISON_TRAIL_CENTER, POISON_TRAIL_FRAMES, PORTAL_CENTER, PORTAL_CUT, PORTAL_RADII,
    Pointer, SPRITE, STEP_FRAME_TIME, STEP_IN, STEP_IN_AT, STEP_OUT, TILE, Textures, WAVE_CENTER,
    WAVE_FRAME_TIME, WIND_LOOP,
};
use crate::balance::{self, Rule, shown};
use crate::game::{
    self, Color, Crest, Event, Game, Hit, Kind, Move, Outcome, Piece, Position, Power, Record,
    Square, Status, Stock, Strike,
};
use crate::net;
use crate::orbit;
use crate::settings::{FrameRate, Settings, ZOOM_MAX, ZOOM_MIN};
use crate::sound::{Sfx, Sound};
use crate::theme::{self, rgba};
use menu::Screen;
use table::{
    Aiming, Bless, Burn, CardMenu, DRAW_DELAY, Draw, Freeze, Held, Portal, ROGUE_VIEW_MIN,
    Returning, Seat, ShopSpot, VIEW_MAX, VIEW_MIN, Wave, ice_variant,
};

/// Скольжение фигуры, как у lichess (секунды).
const SLIDE_TIME: f64 = 0.2;
/// Удар в конце хода: выпад к цели и возврат на свою клетку.
const LUNGE_TIME: f64 = 0.3;
/// Доля выпада, на которой фигура касается цели.
const LUNGE_IMPACT: f64 = 0.35;
/// Насколько глубоко фигура «влетает» в клетку цели при выпаде (в клетках).
const LUNGE_DEPTH: f32 = 0.42;
/// Фигура новой волны (rogue-like) проявляется за столько секунд.
const APPEAR_TIME: f64 = 0.6;
/// Клетка-цель под курсором (с 0.58): рамка «дышит» с этим периодом, секунды, от
/// `TARGET_DIM` до полной яркости; на ней — призрак фигуры с прозрачностью `GHOST_ALPHA`.
const TARGET_PULSE: f64 = 0.9;
const TARGET_DIM: f32 = 0.55;
const GHOST_ALPHA: f32 = 0.45;
/// Удары в конце хода идут по очереди через столько секунд.
const STRIKE_GAP: f64 = 0.22;
/// Сколько живут всплывающие цифры урона.
const EFFECT_TIME: f64 = 1.1;
/// Пауза между цифрами на одной клетке (урон и лечение), чтобы они не слипались.
const NOTE_DELAY: f64 = 0.35;
/// Атака и здоровье фигуры под курсором выезжают по бокам от неё.
const STATS_TIME: f64 = 0.18;
/// Вспышка и дрожь цели после удара.
const FLASH_TIME: f64 = 0.35;
/// Растворение убитой фигуры.
const FADE_TIME: f64 = 0.3;
/// Компьютер (волны rogue-like) кончает ход не мгновенно — так за партией легче следить.
const COMPUTER_DELAY: f64 = 0.45;
/// Сколько висит табличка с итогом партии.
const BANNER_TIME: f64 = 3.0;
/// Текстура целиком.
const UV: Rect = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));

/// Фигура, которую взяли мышью.
struct Drag {
    from: Square,
    press: Pos2,
    pointer: Pos2,
    /// От курсора до основания фигуры: фигура висит так, как её взяли, и встаёт на
    /// клетку под основанием.
    grab: Vec2,
    /// Мышь сдвинулась достаточно, чтобы считать это перетаскиванием, а не щелчком.
    active: bool,
    was_selected: bool,
}

/// Пешка дошла до последней горизонтали — ждём, во что её превратить.
struct PendingPromotion {
    from: Square,
    to: Square,
    drop: Option<Vec2>,
}

/// Анимация последнего хода: фигура скользит на новую клетку.
struct MoveAnim {
    outcome: Outcome,
    /// Теневая фигура (rogue-like) идёт теневым шагом, а не скользит.
    step: bool,
    start: f64,
    /// Откуда фигура начала движение (в клетках доски).
    from: Vec2,
    slide: f64,
    sounded: bool,
}

impl MoveAnim {
    fn new(outcome: Outcome, from: Vec2, start: f64) -> MoveAnim {
        let distance = (center(outcome.mv.to) - from).length();
        let step = outcome.piece.shade && !outcome.mv.is_drop() && distance >= 0.05;
        let slide = if step {
            (STEP_IN_AT + STEP_IN) as f64 * STEP_FRAME_TIME
        } else if distance < 0.05 {
            0.0
        } else {
            SLIDE_TIME
        };
        MoveAnim {
            outcome,
            step,
            start,
            from,
            slide,
            sounded: false,
        }
    }

    /// Когда анимация кончится (секунды от начала).
    fn end(&self) -> f64 {
        self.slide
    }

    /// Где сейчас нарисовать походившую фигуру (в клетках доски).
    fn piece_at(&self, now: f64) -> Vec2 {
        let t = now - self.start;
        let to = center(self.outcome.mv.to);
        if t < self.slide {
            return self.from + (to - self.from) * ease_in_out((t / self.slide) as f32);
        }
        to
    }

    /// Ладья при рокировке едет одновременно с королём.
    fn rook_at(&self, now: f64) -> Option<(Square, Vec2)> {
        let (from, to) = self.outcome.rook?;
        let u = if self.slide > 0.0 {
            ((now - self.start) / self.slide).clamp(0.0, 1.0) as f32
        } else {
            1.0
        };
        Some((
            to,
            center(from) + (center(to) - center(from)) * ease_in_out(u),
        ))
    }

    /// Клетки, фигуры на которых рисует сама анимация.
    fn hides(&self, sq: Square) -> bool {
        sq == self.outcome.mv.to || self.outcome.rook.is_some_and(|(_, to)| to == sq)
    }
}

/// Разряд молнии электрической фигуры между двумя фигурами (с 0.92): мгновенная ломаная
/// вспышка от фигуры на клетке `from` к фигуре на `to`; `lift` — высота точек над землёй
/// (середина фигур), пиксели арта.
struct Zap {
    from: Square,
    to: Square,
    lift: [f32; 2],
    start: f64,
}

/// Сколько держится разряд молнии и как часто перескакивает его ломаная.
const ZAP_TIME: f64 = 0.28;
const ZAP_FLICKER: f64 = 0.05;
/// Молния: голубое свечение и белое ядро.
const ZAP_GLOW: Color32 = Color32::from_rgb(0x5f, 0xd7, 0xff);
const ZAP_CORE: Color32 = Color32::from_rgb(0xf4, 0xff, 0xff);
/// Заряд над головой электрической фигуры — жёлтая молния.
const PIXEL_BOLT: Color32 = Color32::from_rgb(0xff, 0xd8, 0x40);

/// Удар фигуры в конце хода: выпад к цели, в момент касания — цифра урона.
struct StrikeAnim {
    strike: Strike,
    start: f64,
    sounded: bool,
}

impl StrikeAnim {
    /// Когда удар достаёт цель.
    fn impact(&self) -> f64 {
        self.start + LUNGE_TIME * LUNGE_IMPACT
    }

    fn end(&self) -> f64 {
        self.start + LUNGE_TIME
    }

    /// Убийца встаёт на клетку цели (партия «с перемещением») и ещё не дошёл до неё — до
    /// начала удара он стоит на своей клетке.
    fn moving(&self, now: f64) -> bool {
        self.strike.advanced && now < self.end()
    }

    /// Где убийца (в клетках доски): до удара — на своей клетке, потом идёт на клетку цели.
    fn killer_at(&self, now: f64) -> Vec2 {
        let (from, to) = (center(self.strike.from), center(self.strike.hit.square));
        let u = ((now - self.start) / LUNGE_TIME).clamp(0.0, 1.0);
        from + (to - from) * ease_in_out(u as f32)
    }

    /// Сдвиг атакующего во время выпада к выжившей цели (в клетках).
    fn lunge(&self, now: f64) -> Vec2 {
        let u = (now - self.start) / LUNGE_TIME;
        // Круговой удар варвара — без выпада (анимация будет своя).
        if self.strike.advanced || self.strike.whirl || !(0.0..1.0).contains(&u) {
            return Vec2::ZERO;
        }
        (center(self.strike.hit.square) - center(self.strike.from)) * (lunge_reach(u) * LUNGE_DEPTH)
    }
}

/// Что за цифра всплывает над клеткой.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mark {
    Hit,
    Heal,
}

/// След хода: цифра урона или лечения, вспышка, дрожь или растворение погибшей фигуры.
struct Effect {
    square: Square,
    /// Кому досталось (до удара) — чтобы растворить погибшую фигуру.
    victim: Option<Piece>,
    amount: i32,
    killed: bool,
    mark: Mark,
    /// Может быть в будущем: тогда надпись появится позже.
    start: f64,
    /// Сколько цифр на этой клетке ещё висит, когда появляется эта: она встаёт над ними.
    stack: u8,
}

impl Effect {
    fn hit(hit: Hit, start: f64) -> Effect {
        Effect {
            square: hit.square,
            victim: Some(hit.target),
            amount: hit.damage,
            killed: hit.killed,
            mark: Mark::Hit,
            start,
            stack: 0,
        }
    }

    fn note(square: Square, mark: Mark, amount: i32, start: f64) -> Effect {
        Effect {
            square,
            victim: None,
            amount,
            killed: false,
            mark,
            start,
            stack: 0,
        }
    }

    /// Ставит цифру над теми, что на той же клетке ещё будут видны в момент её появления.
    fn stacked(self, effects: &[Effect]) -> Effect {
        let stack = effects
            .iter()
            .filter(|e| e.square == self.square && e.start <= self.start)
            .filter(|e| self.start - e.start < EFFECT_TIME)
            .map(|e| e.stack + 1)
            .max()
            .unwrap_or(0);
        Effect { stack, ..self }
    }

    /// Фигура ранена, но жива — вспышка и дрожь.
    fn wounds(&self) -> bool {
        self.mark == Mark::Hit && !self.killed && self.amount > 0
    }
}

/// Фигура, которую надо нарисовать: где стоит и насколько видна. Фигуры собираются
/// в список, чтобы рисовать их от дальних к ближним.
struct Figure {
    /// Основание фигуры, в клетках доски.
    at: Vec2,
    piece: Piece,
    alpha: f32,
    /// Растворяясь, погибшая фигура растёт.
    grow: f32,
    /// Дрожь после удара, в долях клетки.
    shake: f32,
    shadow: bool,
    /// Фигура обмена в портале под ней.
    sink: Option<Sink>,
    /// Кадр слона с крыльями — слон исцеляет.
    bless: Option<usize>,
    /// Кадр огненной фигуры (лава) и электрической.
    lava: Option<usize>,
    spark: Option<usize>,
    /// Кадр вихря воздушной фигуры (rogue-like).
    wind: Option<usize>,
    /// Кадр лужи и капель ядовитой фигуры волн (rogue-like).
    poison: Option<usize>,
    /// Теневая фигура (rogue-like): кадр ауры, исчезновения или появления (`Dark`).
    dark: Option<Dark>,
    /// Лёд на фигуре: кадр заморозки (иней и глыба), вариант глыбы и сколько льда
    /// осталось — оттаивая, он тает.
    ice: Option<Ice>,
}

/// Кадр теневой фигуры: аура, исчезновение и появление теневого шага.
#[derive(Clone, Copy)]
enum Dark {
    Aura(usize),
    Out(usize),
    In(usize),
}

#[derive(Clone, Copy)]
struct Ice {
    frame: usize,
    variant: usize,
    alpha: f32,
}

impl Figure {
    fn new(at: Vec2, piece: Piece) -> Figure {
        Figure {
            at,
            piece,
            alpha: 1.0,
            grow: 1.0,
            shake: 0.0,
            shadow: true,
            sink: None,
            bless: None,
            lava: None,
            spark: None,
            wind: None,
            poison: None,
            dark: None,
            ice: None,
        }
    }
}

/// Метеорит падает вдвое быстрее, чем идут кадры взрыва (с 0.85, просьба пользователя): кадр
/// полёта — столько секунд.
const METEOR_FALL_TIME: f64 = METEOR_FRAME_TIME / 2.0;

/// Когда метеорит, начавший падать в `start`, касается клетки.
fn meteor_impact(start: f64) -> f64 {
    start + METEOR_IMPACT as f64 * METEOR_FALL_TIME
}

/// Кадр метеорита: полёт — по `METEOR_FALL_TIME`, взрыв — по `METEOR_FRAME_TIME`.
fn meteor_frame(start: f64, now: f64) -> usize {
    let impact = meteor_impact(start);
    if now < impact {
        ((now - start).max(0.0) / METEOR_FALL_TIME) as usize
    } else {
        METEOR_IMPACT + ((now - impact) / METEOR_FRAME_TIME) as usize
    }
}

/// Погасший кратер исчезает не сразу, а через столько секунд после шагов.
const CRATER_FADE: f64 = 0.3;

/// Метеориты магов падают один за другим через столько секунд (rogue-like, с 0.83).
const METEOR_GAP: f64 = 0.3;

/// Отравленная фигура — тёмно-зелёный череп над ней (rogue-like, с 0.79).
const POISON_SKULL: Color32 = Color32::from_rgb(0x2e, 0x8c, 0x1e);
/// Череп на могиле зомби (rogue-like).
const GRAVE_SKULL: Color32 = Color32::from_rgb(0xb8, 0xc4, 0xa0);

/// Лава в огненной фигуре: насыщение (трещины растут) или взрыв (разгораются, огненная волна,
/// гаснут).
struct LavaAnim {
    square: Square,
    start: f64,
    blast: bool,
}

impl LavaAnim {
    fn length(&self) -> f64 {
        let frames = if self.blast {
            LAVA_FRAMES - LAVA_IGNITE
        } else {
            LAVA_SPREAD
        };
        frames as f64 * LAVA_FRAME_TIME
    }
}

/// Насколько фигура ушла в портал (`depth`, пиксели арта) и как раскрыто его кольцо
/// (`scale`): видна только часть фигуры выше передней кромки кольца.
#[derive(Clone, Copy)]
struct Sink {
    depth: f32,
    scale: f32,
}

/// Где на экране доска и как перевести клетки в точки и обратно. Доска изометрическая,
/// как в Into the Breach: клетки — ромбы 56×42 пикселя арта, свои фигуры внизу слева.
#[derive(Clone, Copy)]
struct BoardGeom {
    /// Поле под доску: в нём ловится мышь.
    area: Rect,
    /// Левый верхний угол картинки доски.
    origin: Pos2,
    /// Точек экрана на пиксель арта.
    scale: f32,
    /// Во сколько раз увеличены текстуры (целое число физических пикселей на пиксель арта).
    texture_scale: u32,
    /// Физических пикселей на точку экрана.
    ppp: f32,
    flipped: bool,
}

impl BoardGeom {
    /// Картинка доски: клетки с боками и подписи.
    fn rect(&self) -> Rect {
        Rect::from_min_size(self.origin, BOARD * self.scale)
    }

    /// Мера для надписей и окон — примерно сторона клетки плоской доски.
    fn unit(&self) -> f32 {
        TILE.y * self.scale
    }

    /// Координаты в клетках по осям изометрии: первая растёт вправо-вниз, вторая — влево-вниз.
    /// Преобразование обратно себе, поэтому годится и для обратного перевода.
    fn iso(&self, p: Vec2) -> Vec2 {
        if self.flipped {
            vec2(8.0 - p.x, p.y)
        } else {
            vec2(p.x, 8.0 - p.y)
        }
    }

    /// Точка на верхних гранях клеток. `p` — координаты в клетках: x вдоль горизонтали
    /// от a, y вверх от первой горизонтали.
    fn to_screen(&self, p: Vec2) -> Pos2 {
        let c = self.iso(p);
        let art = vec2(
            (c.x - c.y) * TILE.x / 2.0 + BOARD.x / 2.0,
            (c.x + c.y) * TILE.y / 2.0,
        );
        self.origin + art * self.scale
    }

    fn to_board(&self, pos: Pos2) -> Vec2 {
        let art = (pos - self.origin) / self.scale;
        let diff = (art.x - BOARD.x / 2.0) / (TILE.x / 2.0);
        let sum = art.y / (TILE.y / 2.0);
        self.iso(vec2(sum + diff, sum - diff) / 2.0)
    }

    /// Точка экрана в пикселях арта — от левого верхнего угла картинки доски.
    fn to_art(&self, pos: Pos2) -> Vec2 {
        (pos - self.origin) / self.scale
    }

    fn art_to_screen(&self, art: Vec2) -> Pos2 {
        self.origin + art * self.scale
    }

    /// Точка на ближайшем физическом пикселе: картинка, поставленная в неё, не размывается
    /// (фигуры и карты в движении стоят между пикселями).
    fn snap(&self, p: Pos2) -> Pos2 {
        pos2(
            (p.x * self.ppp).round() / self.ppp,
            (p.y * self.ppp).round() / self.ppp,
        )
    }
    /// Чем больше, тем ближе к зрителю: ближние фигуры рисуются поверх дальних.
    fn depth(&self, p: Vec2) -> f32 {
        let c = self.iso(p);
        c.x + c.y
    }

    fn ground(&self, sq: Square) -> Pos2 {
        self.to_screen(center(sq))
    }

    /// Ромб верхней грани клетки — место для подсветок.
    fn tile_rect(&self, sq: Square) -> Rect {
        Rect::from_center_size(self.ground(sq), TILE * self.scale)
    }

    /// Холст фигуры, стоящей в точке `ground`, увеличенной в `grow` раз.
    fn sprite_rect(&self, ground: Pos2, grow: f32) -> Rect {
        Rect::from_min_size(
            ground - ANCHOR * self.scale * grow,
            SPRITE * self.scale * grow,
        )
    }

    /// Клетка, на ромб которой указывает точка (фигуры не учитываются).
    fn square_at(&self, pos: Pos2) -> Option<Square> {
        let v = self.to_board(pos);
        if !(0.0..8.0).contains(&v.x) || !(0.0..8.0).contains(&v.y) {
            return None;
        }
        game::square(v.x as i8, v.y as i8)
    }
}

pub struct BattleChessApp {
    /// Меню, выбор колоды, коллекция карт или партия.
    screen: Screen,
    /// Предупреждения о `balance.ini` из последнего чтения — их показывает главное меню.
    balance_warnings: Vec<String>,
    /// Что произошло с последним вставленным или скопированным кодом колоды (текст и успех).
    deck_message: Option<(String, bool)>,
    /// Окно было в фокусе в прошлом кадре: вернулись из Блокнота — баланс читаем заново.
    window_focused: bool,
    /// Открытая страница коллекции карт.
    collection_page: usize,
    game: Game,
    art: Art,
    sfx: Sfx,
    /// Сторона компьютера (волны rogue-like) и когда ему стало можно кончить ход.
    computer: Option<Color>,
    computer_since: Option<f64>,
    selected: Option<Square>,
    hovered: Option<Square>,
    /// Фигура под курсором, чьи атака и здоровье показаны, и когда они начали выезжать.
    stats_for: Option<(Square, f64)>,
    /// Карта в руке под курсором.
    card_hover: Option<(Seat, usize)>,
    /// С какого времени эта карта под курсором — она выезжает вверх.
    card_hover_since: f64,
    /// Колода под курсором — над ней табличка с числом карт — и с какого времени.
    deck_hover: Option<(Seat, f64)>,
    /// Курсор: какой показать и во сколько раз увеличить (целое, как у доски).
    pointer: Pointer,
    cursor_scale: u32,
    /// Карта, которую держат мышью, и карты, возвращающиеся в руку.
    held: Option<Held>,
    returning: Vec<Returning>,
    burns: Vec<Burn>,
    draws: Vec<Draw>,
    /// Карта с выбором фигуры, ждущая цель, огненные волны, обмены и исцеления.
    aiming: Option<Aiming>,
    waves: Vec<Wave>,
    portals: Vec<Portal>,
    blesses: Vec<Bless>,
    /// Заморозки, тающий лёд (клетка и когда начал таять) и где лёд был в прошлом кадре:
    /// клетки замёрзших фигур и клетки под снегом.
    freezes: Vec<Freeze>,
    thaws: Vec<(Square, f64)>,
    ice_seen: (u64, u64),
    /// Кнопка «Конец хода»: курсор над ней и нажата ли она.
    button_hover: bool,
    button_down: bool,

    /// Мультиплеер: соединение с сервером, цвет соперника по сети (в сетевой партии), адрес
    /// в поле ввода, строка состояния на экране подключения, заметка в партии (соперник ушёл,
    /// связь потеряна) и ходы соперника, ждущие своей очереди.
    net: Option<net::Link>,
    remote: Option<Color>,
    net_address: String,
    net_status: String,
    net_note: Option<String>,
    net_queue: VecDeque<String>,
    /// Сервер посадил нас за чёрных: партия начнётся, когда белые пришлют правила.
    net_pending: Option<Color>,
    /// Фигура запаса под курсором и выбранная для выставления.
    reserve_hover: Option<(Seat, Stock)>,
    /// Rogue-like: место магазина под курсором.
    shop_hover: Option<ShopSpot>,
    /// Rogue-like (с 0.126): магазин свёрнут — доска и карты свободны, наверху кнопка «Магазин»;
    /// свёрнут сам ради цели карты (развернётся, когда карту сыграют или отменят); курсор над
    /// «Магазином».
    shop_folded: bool,
    fold_for_card: bool,
    unfold_hover: bool,
    /// Rogue-like (с 0.126): карта руки, у которой в магазине открыто меню «Сыграть» / «Продать»,
    /// и плашка меню под курсором.
    card_menu: Option<usize>,
    card_menu_hover: Option<CardMenu>,
    /// Rogue-like: герб под курсором (в колонке слева или на прилавке) и его место на экране — для
    /// подсказки.
    crest_hover: Option<(game::Ware, Rect)>,
    /// Rogue-like: выбранная карта в руке (место), карта под курсором, первая фигура зеркала.
    power_pick: Option<usize>,
    power_hover: Option<usize>,
    mirror_from: Option<Square>,
    /// Rogue-like: фигура запаса, которую несут в зону продажи, и где курсор.
    sell_drag: Option<(Stock, Pos2)>,
    /// Rogue-like: герб, выбранный на продажу, и курсор над кнопкой «Продать».
    crest_sell: Option<usize>,
    crest_sell_hover: bool,
    drop_pick: Option<Stock>,
    drag: Option<Drag>,
    promotion: Option<PendingPromotion>,
    anim: Option<MoveAnim>,
    /// Удары в конце хода — по очереди, после хода фигурой.
    strikes: Vec<StrikeAnim>,
    /// Лава в огненных фигурах: насыщение и взрывы.
    lavas: Vec<LavaAnim>,
    /// Rogue-like: фигуры волны проявляются на своих клетках (клетка, когда начали).
    appears: Vec<(Square, f64)>,
    /// Rogue-like: шаги фигур волн вперёд (откуда, куда, когда начали).
    marches: Vec<(Square, Square, f64)>,
    /// Rogue-like: следы ядовитых фигур, шагнувших вперёд (откуда, куда, когда начали).
    trails: Vec<(Square, Square, f64)>,
    /// Rogue-like: урон яда и метеоритов магов — когда покажется; до тех пор фигура прежняя.
    due_hits: Vec<(Hit, f64)>,
    /// Rogue-like: метеориты магов (клетка, когда начали падать, прозвучал ли удар, убил ли).
    meteors: Vec<(Square, f64, bool, bool)>,
    /// Разряды молний между фигурами.
    zaps: Vec<Zap>,
    /// Горящие кратеры метеоритов: клетка, когда загорелся (в миг касания), когда гаснет
    /// (`None` — пока горит в позиции).
    craters: Vec<(Square, f64, Option<f64>)>,
    /// Волны метеоритов крестом: клетка, когда начались.
    meteor_waves: Vec<(Square, f64)>,
    /// Начало партии: от него идут кадры электричества (вступление, потом повтор).
    spark_since: f64,
    /// Время текущего кадра.
    now: f64,
    /// Сколько записей истории уже показано: удары новых концов хода ещё не начались.
    shown_history: usize,
    effects: Vec<Effect>,
    ended_at: Option<f64>,
    flipped: bool,
    /// Осмотр доски с камерой вокруг объёмных клеток. Пока включён, ходы не играются.
    orbit: Option<orbit::Orbit>,
    /// Настройки и то, что из них уже сохранено на диск.
    settings: Settings,
    saved: Settings,
    /// Включён ли VSync в этом запуске (меняется только перезапуском).
    vsync: bool,
    /// Когда начинать следующий кадр при ограничении частоты без VSync.
    next_frame: Option<Instant>,
    /// Сколько текущий кадр ждал ограничителя, секунды.
    slept: f32,
    fps: FpsMeter,
}

/// Счётчик кадров: сколько кадров нарисовано за последнюю секунду и сколько
/// процессорного времени уходит на кадр.
#[derive(Default)]
struct FpsMeter {
    frames: VecDeque<f64>,
    /// Сглаженное время кадра, секунды.
    cpu: f32,
}

impl FpsMeter {
    fn record(&mut self, now: f64, cpu: Option<f32>) {
        self.frames.push_back(now);
        while self.frames.front().is_some_and(|&t| now - t > 1.0) {
            self.frames.pop_front();
        }
        if let Some(cpu) = cpu {
            self.cpu = self.cpu * 0.9 + cpu * 0.1;
        }
    }

    fn fps(&self) -> f64 {
        match (self.frames.front(), self.frames.back()) {
            (Some(&first), Some(&last)) if last > first => {
                (self.frames.len() - 1) as f64 / (last - first)
            }
            _ => 0.0,
        }
    }
}

impl BattleChessApp {
    pub fn new(cc: &eframe::CreationContext<'_>, settings: Settings) -> BattleChessApp {
        theme::apply(&cc.egui_ctx);
        let balance_warnings = balance::reload();
        crate::editor::start(&cc.egui_ctx);
        let mut app = BattleChessApp {
            screen: Screen::Menu,
            collection_page: 0,
            balance_warnings,
            deck_message: None,
            window_focused: true,
            game: Game::from_position(Position::start().without_cards()),
            art: Art::new(),
            sfx: Sfx::new(),
            computer: None,
            computer_since: None,

            selected: None,
            hovered: None,
            stats_for: None,
            card_hover: None,
            card_hover_since: 0.0,
            deck_hover: None,
            pointer: Pointer::Default,
            cursor_scale: 2,
            held: None,
            returning: Vec::new(),
            burns: Vec::new(),
            draws: Vec::new(),
            aiming: None,
            waves: Vec::new(),
            portals: Vec::new(),
            blesses: Vec::new(),
            freezes: Vec::new(),
            thaws: Vec::new(),
            ice_seen: (0, 0),
            button_hover: false,
            button_down: false,

            reserve_hover: None,
            shop_hover: None,
            shop_folded: false,
            fold_for_card: false,
            unfold_hover: false,
            card_menu: None,
            card_menu_hover: None,
            crest_hover: None,
            power_pick: None,
            power_hover: None,
            mirror_from: None,
            sell_drag: None,
            crest_sell: None,
            crest_sell_hover: false,
            net: None,
            remote: None,
            net_address: net::saved_address(),
            net_status: String::new(),
            net_note: None,
            net_queue: VecDeque::new(),
            net_pending: None,
            drop_pick: None,
            drag: None,
            promotion: None,
            anim: None,
            strikes: Vec::new(),
            lavas: Vec::new(),
            appears: Vec::new(),
            marches: Vec::new(),
            trails: Vec::new(),
            due_hits: Vec::new(),
            meteors: Vec::new(),
            zaps: Vec::new(),
            craters: Vec::new(),
            meteor_waves: Vec::new(),
            spark_since: 0.0,
            now: 0.0,
            shown_history: 0,
            effects: Vec::new(),
            ended_at: None,
            flipped: false,
            orbit: None,
            settings,
            saved: settings,
            vsync: settings.frame_rate.vsync(),
            next_frame: None,
            slept: 0.0,
            fps: FpsMeter::default(),
        };
        app.new_rogue_game();
        app
    }

    /// Читает `balance.ini` заново и, если цены карт изменились, перерисовывает их лица.
    fn reload_balance(&mut self) {
        self.balance_warnings = balance::reload();
        self.art.sync_costs();
    }

    /// Новая партия — rogue-like (с 0.108 единственный режим против компьютера): игрок — белые,
    /// у компьютера фигуры приходят волнами (`Position::rogue_start`); первая волна проявляется
    /// на глазах.
    pub(super) fn new_rogue_game(&mut self) {
        self.leave_network();
        self.reload_balance();
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0x5EED, |d| d.as_nanos() as u64);
        self.game =
            Game::from_position(Position::rogue_start(seed).with_advance(self.settings.advance));
        self.computer = Some(Color::Black);
        self.flipped = false;
        self.orbit = None;
        self.reset_interaction();
        let now = self.now;
        self.spark_since = now;
        let wave: Vec<Square> = self
            .game
            .position()
            .pieces()
            .filter(|(_, p)| p.color == Color::Black)
            .map(|(sq, _)| sq)
            .collect();
        self.appears = wave.into_iter().map(|sq| (sq, now)).collect();
        self.sfx.play(Sound::Notify);
    }

    /// Сдвиг фигуры на `sq`, которая ещё шагает туда с прошлой клетки (rogue-like волна): до
    /// начала шага — на прежней клетке.
    /// Откуда шагает фигура, стоящая на `sq` (rogue-like волна), — там её удары до шага.
    fn march_origin(&self, sq: Square) -> Square {
        self.marches
            .iter()
            .find(|&&(_, to, _)| to == sq)
            .map_or(sq, |&(from, _, _)| from)
    }

    fn march_shift(&self, sq: Square, now: f64) -> Vec2 {
        self.marches.iter().find(|&&(_, to, _)| to == sq).map_or(
            Vec2::ZERO,
            |&(from, to, start)| {
                let u = ((now - start) / SLIDE_TIME).clamp(0.0, 1.0) as f32;
                (center(from) - center(to)) * (1.0 - ease_in_out(u))
            },
        )
    }

    /// Насколько проявилась фигура на `sq` (rogue-like волна): 0 — ещё в тумане, 1 — вся.
    fn appear_alpha(&self, sq: Square, now: f64) -> f32 {
        self.appears
            .iter()
            .find(|&&(s, _)| s == sq)
            .map_or(1.0, |&(_, start)| {
                ((now - start) / APPEAR_TIME).clamp(0.0, 1.0) as f32
            })
    }

    /// Сбрасывает выделение, анимации и раздумья компьютера после смены позиции.
    fn reset_interaction(&mut self) {
        self.selected = None;
        self.drop_pick = None;
        self.power_pick = None;
        self.mirror_from = None;
        self.sell_drag = None;
        self.crest_sell = None;
        self.card_menu = None;
        self.drag = None;
        self.promotion = None;
        self.anim = None;
        self.effects.clear();
        self.burns.clear();
        self.draws.clear();
        self.waves.clear();
        self.portals.clear();
        self.blesses.clear();
        self.reset_ice();
        self.aiming = None;
        self.button_down = false;
        self.returning.clear();
        self.held = None;
        self.card_hover = None;
        self.computer_since = None;
        self.strikes.clear();
        self.lavas.clear();
        self.appears.clear();
        self.marches.clear();
        self.trails.clear();
        self.due_hits.clear();
        self.meteors.clear();
        self.zaps.clear();
        self.craters.clear();
        self.meteor_waves.clear();
        self.shown_history = self.game.history().len();
        self.ended_at = None;
    }

    /// Отменяет ход целиком — до его начала; если ход только начался — предыдущий.
    /// Против компьютера отменяется и его ход, чтобы снова ходил человек. В сетевой партии
    /// отмены нет.
    fn undo(&mut self) {
        if self.remote.is_some() || !self.game.undo() {
            return;
        }
        loop {
            let pos = self.game.position();
            // «Выиграть раунд» отменяется вместе с концом хода после него.
            let win = self.game.history().last().is_some_and(|r| {
                matches!(r.event, Event::Rogue(game::RogueAction::WinRound { .. }))
            });
            let mid_turn = pos.moved() || pos.cards_played() > 0 || win;
            if !(mid_turn || self.computer == Some(pos.side())) || !self.game.undo() {
                break;
            }
        }
        self.reset_interaction();
    }

    /// Кнопка «Конец хода» (или Space, Enter): соперник начинает ход с карты из колоды.
    fn end_turn(&mut self, now: f64) {
        if self.game.is_over()
            || !self.game.can_end_turn()
            || self.aiming_card()
            || self.promotion.is_some()
        {
            return;
        }
        let mover = self.game.position().side();
        let next = mover.opponent();
        let hand = self.game.position().cards(next).hand().len();
        if self.game.end_turn() {
            self.selected = None;
            self.note_draw(next, hand, now);
            self.send_own(mover, net::END_TURN.to_owned());
        }
    }

    fn human_to_move(&self) -> bool {
        let side = Some(self.game.position().side());
        !self.game.is_over() && side != self.computer && side != self.remote
    }

    /// В сетевой партии своё действие (не соперника) уходит сопернику.
    fn send_own(&self, mover: Color, line: String) {
        if let (Some(link), Some(remote)) = (&self.net, self.remote)
            && mover != remote
        {
            link.send(&line);
        }
    }

    fn has_move(&self, from: Square, to: Square) -> bool {
        self.game
            .legal_moves()
            .iter()
            .any(|m| m.from == from && m.to == to && !m.is_drop())
    }

    /// Подсвеченная клетка, куда сейчас можно пойти выбранной фигурой или выставить фигуру
    /// запаса.
    fn is_target(&self, sq: Square) -> bool {
        if let Some(card) = self.picked_card() {
            return self.card_squares(card).contains(&sq);
        }
        match (self.drop_pick, self.selected) {
            (Some(stock), _) => self
                .game
                .legal_moves()
                .iter()
                .any(|m| m.kind == stock.drop_kind() && m.to == sq),
            (None, Some(from)) => from != sq && self.has_move(from, sq),
            (None, None) => false,
        }
    }

    /// Rogue-like: выбранная карта в руке.
    fn picked_card(&self) -> Option<game::Card> {
        self.game.position().hand().get(self.power_pick?).copied()
    }

    /// Rogue-like: карта на месте `slot` руки.
    fn picked_card_at(&self, slot: usize) -> Option<game::Card> {
        self.game.position().hand().get(slot).copied()
    }

    /// Rogue-like: куда сейчас сыграть выбранную карту (у зеркала после первой фигуры — какой
    /// второй её сделать).
    fn card_squares(&self, card: game::Card) -> Vec<Square> {
        self.game.position().card_squares(card, self.mirror_from)
    }

    /// Rogue-like: щелчок по карте в руке — сразу играется, если целей не просит, иначе
    /// выбирается (повторный щелчок снимает выбор).
    fn click_card(&mut self, slot: usize) {
        self.mirror_from = None;
        let Some(card) = self.game.position().hand().get(slot).copied() else {
            return;
        };
        if !self.hand_usable(slot) {
            self.power_pick = None;
            return;
        }
        if card.picks() == 0 {
            self.power_pick = None;
            if self.game.use_card(slot, &[]) {
                self.sfx.play(Sound::Notify);
            }
            return;
        }
        self.power_pick = (self.power_pick != Some(slot)).then_some(slot);
    }

    /// Rogue-like: магазин открыт, и показывать его уже можно — удары, шаги и ход доиграли.
    fn shop_ready(&self) -> bool {
        self.game.position().shop_open()
            && self.anim.is_none()
            && self.strikes.is_empty()
            && self.due_hits.is_empty()
            && self.marches.is_empty()
            && self.lavas.is_empty()
    }

    /// Rogue-like: панель магазина на экране — он готов и не свёрнут (с 0.126).
    fn shop_shown(&self) -> bool {
        self.shop_ready() && !self.shop_folded
    }

    /// Клетка-цель под курсором, когда ходит человек.
    fn hovered_target(&self) -> Option<Square> {
        self.hovered
            .filter(|&sq| self.promotion.is_none() && self.human_to_move() && self.is_target(sq))
    }

    /// Призрак на клетке-цели под курсором: выбранная фигура или фигура запаса. На клетке со
    /// своей фигурой (обмен при расстановке) призрака нет.
    fn ghost(&self) -> Option<(Square, Piece)> {
        let sq = self.hovered_target()?;
        let pos = self.game.position();
        if pos.piece(sq).is_some() {
            return None;
        }
        let piece = match self.drop_pick {
            Some(stock) => stock.piece(pos.side()),
            None => pos.piece(self.selected?)?,
        };
        Some((sq, piece))
    }

    fn play(&mut self, mv: Move, drop: Option<Vec2>, now: f64) {
        self.advance_animation(now, true);
        self.finish_portals();
        let mover = self.game.position().side();
        let next = mover.opponent();
        let hand = self.game.position().cards(next).hand().len();
        let Some(outcome) = self.game.play(mv) else {
            return;
        };
        self.send_own(mover, net::encode_move(mv));
        self.selected = None;
        self.promotion = None;
        // Вставшая в ледяную зону фигура (и ладья рокировки) замерзает, когда доедет.
        let pos = self.game.position();
        let iced: Vec<Square> = [Some(outcome.mv.to), outcome.rook.map(|(_, to)| to)]
            .into_iter()
            .flatten()
            .filter(|&sq| pos.piece(sq).is_some_and(|p| p.is_frozen()))
            .collect();
        let anim = MoveAnim::new(outcome, drop.unwrap_or(center(mv.from)), now);
        if mv.is_drop() {
            self.sfx.play(Sound::Notify);
        }
        self.freeze_arrivals(&iced, now + anim.end());
        self.anim = Some(anim);
        // Новый ход начинается с карты из колоды.
        self.note_draw(next, hand, now + DRAW_DELAY);
    }

    /// Звуки и эффекты анимации в нужный момент; `finish` доигрывает её сразу.
    fn advance_animation(&mut self, now: f64, finish: bool) {
        let Some(anim) = &mut self.anim else {
            return;
        };
        let t = if finish {
            f64::INFINITY
        } else {
            now - anim.start
        };
        if !anim.sounded {
            anim.sounded = true;
            if !finish {
                self.sfx.play(Sound::Move);
            }
        }
        if t >= anim.end() {
            self.anim = None;
        }
    }

    /// Новые концы хода в истории: их удары встают в очередь — после хода фигурой и уже
    /// идущих ударов, по одному через `STRIKE_GAP`. Цифры урона и лечения появятся в момент
    /// касания, цель «Воина из Льда» замёрзнет тогда же.
    fn queue_strikes(&mut self, now: f64) {
        let history = self.game.history();
        if history.len() < self.shown_history {
            self.shown_history = history.len();
            return;
        }
        let mut start = self
            .anim
            .as_ref()
            .map_or(now, |a| a.start + a.end())
            .max(now);
        if let Some(last) = self.strikes.last() {
            start = start.max(last.start + STRIKE_GAP);
        }
        let mut chilled = Vec::new();
        // Круговой удар варвара задевает всех разом: его удары — в один миг, звук — один.
        let mut whirl_from = None;
        for record in &history[self.shown_history..] {
            let struck = !record.strikes().is_empty();
            for strike in record.strikes() {
                // Теневая ушла порталом: сначала обмен, потом удар по вставшей на её место.
                if let Some((other, shade)) = strike.swap {
                    self.portals.push(Portal::swap(
                        [strike.hit.square, other],
                        [shade, strike.hit.target],
                        start,
                    ));
                    start += Portal::length();
                }
                let joined = strike.whirl && whirl_from == Some(strike.from);
                if joined {
                    start -= STRIKE_GAP;
                }
                whirl_from = strike.whirl.then_some(strike.from);
                let anim = StrikeAnim {
                    strike: strike.clone(),
                    start,
                    sounded: joined,
                };
                // Не полученный теневой фигурой удар — без цифры.
                if !strike.dodged {
                    let hit = Effect::hit(strike.hit, anim.impact()).stacked(&self.effects);
                    self.effects.push(hit);
                }
                if strike.feast > 0 {
                    // Лечится ударивший — на своей клетке (встав на клетку убитой — там).
                    let at = if strike.advanced {
                        strike.hit.square
                    } else {
                        strike.from
                    };
                    self.effects.push(Effect::note(
                        at,
                        Mark::Heal,
                        strike.feast,
                        anim.impact() + NOTE_DELAY,
                    ));
                }
                if strike.chilled {
                    chilled.push((strike.hit.square, anim.impact()));
                }
                // Ледяная цель: ударивший замерзает в момент касания.
                if strike.rebound {
                    chilled.push((strike.from, anim.impact()));
                }
                // Молния электрической фигуры задевает фигуры вокруг цели в тот же миг (между
                // ними проскакивают разряды).
                for &hit in strike.splash.iter().map(|j| &j.hit) {
                    let spark = Effect::hit(hit, anim.impact()).stacked(&self.effects);
                    self.effects.push(spark);
                }
                let lift = |p: Piece| self.art.height(p.color, p.kind) * 0.55;
                for jump in &strike.splash {
                    let source = if jump.from == strike.hit.square {
                        Some(strike.hit.target)
                    } else {
                        strike
                            .splash
                            .iter()
                            .find(|j| j.hit.square == jump.from)
                            .map(|j| j.hit.target)
                    };
                    self.zaps.push(Zap {
                        from: jump.from,
                        to: jump.hit.square,
                        lift: [source.map_or(20.0, lift), lift(jump.hit.target)],
                        start: anim.impact(),
                    });
                }
                self.strikes.push(anim);
                start += STRIKE_GAP;
            }
            let Some(turn) = record.turn_end() else {
                continue;
            };
            // Огненные фигуры разгораются и взрываются по очереди после ударов; огненная волна
            // начинается в кадре трещин `LAVA_WAVE_AT`.
            let mut blasts_end = start;
            // Повторный взрыв той же фигуры (герб огня и двойки) — когда доиграл прежний.
            let mut busy: Vec<(Square, f64)> = Vec::new();
            for blast in &turn.blasts {
                let free = busy
                    .iter()
                    .filter(|b| b.0 == blast.center)
                    .map(|b| b.1)
                    .fold(0.0, f64::max);
                let ignite = (start + LUNGE_TIME).max(free);
                busy.push((
                    blast.center,
                    ignite + (LAVA_FRAMES - LAVA_IGNITE) as f64 * LAVA_FRAME_TIME,
                ));
                self.lavas.push(LavaAnim {
                    square: blast.center,
                    start: ignite,
                    blast: true,
                });
                let wave_at = ignite + (LAVA_WAVE_AT - LAVA_IGNITE) as f64 * LAVA_FRAME_TIME;
                self.waves
                    .push(Wave::new(blast.center, blast.hits.clone(), wave_at).wide(blast.wide));
                blasts_end =
                    blasts_end.max(ignite + (LAVA_FRAMES - LAVA_IGNITE) as f64 * LAVA_FRAME_TIME);
                start = start.max(ignite - LUNGE_TIME) + 2.0 * STRIKE_GAP;
            }
            // Огненные фигуры после взрыва снова насыщаются огнём (повторно взорвавшаяся — раз).
            let mut centers: Vec<Square> = turn.blasts.iter().map(|b| b.center).collect();
            centers.sort_unstable();
            centers.dedup();
            for center in centers {
                if self.game.position().piece(center).is_some_and(|p| p.fire) {
                    self.lavas.push(LavaAnim {
                        square: center,
                        start: blasts_end,
                        blast: false,
                    });
                }
            }
            // Метеориты магов — по очереди, когда доиграли удары и взрывы; урон — в миг касания.
            let mut late = blasts_end;
            for (i, meteor) in turn.meteors.iter().enumerate() {
                let fall = blasts_end + i as f64 * METEOR_GAP;
                let impact = meteor_impact(fall);
                for &hit in &meteor.hits {
                    let effect = Effect::hit(hit, impact).stacked(&self.effects);
                    self.effects.push(effect);
                    self.due_hits.push((hit, impact));
                }
                let killed = meteor.hits.iter().any(|h| h.killed);
                self.meteors.push((meteor.at, fall, false, killed));
                if meteor.wide {
                    self.meteor_waves.push((meteor.at, impact));
                }
                if meteor.crater {
                    self.craters.retain(|c| c.0 != meteor.at);
                    self.craters.push((meteor.at, impact, None));
                }
                late = late.max(impact);
            }
            // Яд отравленных фигур — когда доиграли удары, взрывы и метеориты.
            if !turn.poison.is_empty() {
                let at = late + LUNGE_TIME;
                for &hit in &turn.poison {
                    let effect = Effect::hit(hit, at).stacked(&self.effects);
                    self.effects.push(effect);
                    self.due_hits.push((hit, at));
                }
            }
            // Rogue-like: волны сначала бьют, потом шагают вперёд; новая волна проявляется
            // после шага.
            // Фигуры игрока к расстановке возвращаются на места начала уровня так же.
            let mut march_end = blasts_end;
            if !turn.marched.is_empty() || !turn.returned.is_empty() {
                let march_at = if struck {
                    blasts_end + LUNGE_TIME - STRIKE_GAP
                } else {
                    blasts_end
                };
                for &(from, to) in turn.marched.iter().chain(&turn.returned) {
                    self.marches.push((from, to, march_at));
                    if self.game.position().piece(to).is_some_and(|p| p.venom) {
                        self.trails.push((from, to, march_at));
                    }
                }
                march_end = march_at + SLIDE_TIME;
                start = start.max(march_end);
            }
            for &(square, _) in turn.spawned.iter().chain(&turn.risen) {
                self.appears.push((square, march_end));
            }
            // Шагнувшие в горящий кратер горят, когда дошли.
            for &hit in &turn.burned {
                let effect = Effect::hit(hit, march_end).stacked(&self.effects);
                self.effects.push(effect);
                self.due_hits.push((hit, march_end));
            }
            // Кратеры гаснут к ходу игрока: после шагов этого хода соперника (и к расстановке).
            let player = self.game.position().rogue().map(|r| r.player);
            if Some(record.color()) != player || turn.planning.is_some() {
                for crater in self.craters.iter_mut().filter(|c| c.2.is_none()) {
                    crater.2 = Some(march_end.max(crater.1) + CRATER_FADE);
                }
            }
        }
        self.shown_history = history.len();
        for (sq, at) in chilled {
            self.freeze_arrivals(&[sq], at);
        }
    }

    /// Звук каждого удара — в момент касания; доигравшие удары уходят.
    fn advance_strikes(&mut self, now: f64) {
        for strike in &mut self.strikes {
            if !strike.sounded && now >= strike.impact() {
                strike.sounded = true;
                self.sfx.play(if strike.strike.hit.killed {
                    Sound::Kill
                } else {
                    Sound::Hit
                });
            }
        }
        self.strikes.retain(|s| now < s.end());
    }

    /// Фигура на клетке `sq` такой, какой её ещё видно: до касания удара по ней (или молнии
    /// слона) — прежней.
    fn struck_piece(&self, sq: Square, now: f64) -> Option<Piece> {
        self.strikes
            .iter()
            .filter(|s| now < s.impact())
            .flat_map(|s| s.strike.hits().map(move |h| (s.start, h)))
            .filter(|(_, h)| h.square == sq)
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, h)| h.target)
    }

    /// Кадр лавы огненной фигуры на `sq`: взрыв идёт — кадры разгорания, впереди — ожидание;
    /// огонь снова насыщается — растут трещины (до начала — обычная фигура); иначе — ожидание.
    fn lava_frame(&self, sq: Square, piece: Piece, now: f64) -> Option<usize> {
        if !piece.fire && self.lavas.iter().all(|a| a.square != sq) {
            return None;
        }
        let hold = || {
            let [from, to] = LAVA_HOLD;
            from + (now / LAVA_FRAME_TIME) as usize % (to - from)
        };
        let elapsed = |start: f64| ((now - start) / LAVA_FRAME_TIME) as usize;
        let ends = |a: &LavaAnim| a.start + a.length();
        if let Some(blast) = self
            .lavas
            .iter()
            .find(|a| a.blast && a.square == sq && now < ends(a))
        {
            return Some(if now < blast.start {
                hold()
            } else {
                LAVA_IGNITE + elapsed(blast.start)
            });
        }
        if let Some(charge) = self
            .lavas
            .iter()
            .find(|a| !a.blast && a.square == sq && now < ends(a))
        {
            return (now >= charge.start).then(|| elapsed(charge.start));
        }
        piece.fire.then(hold)
    }

    /// Кадр тёмной ауры теневой фигуры: вступление с начала партии, потом повтор (сдвинут по
    /// клетке).
    fn dark_frame(&self, sq: Square, piece: Piece, now: f64) -> Option<usize> {
        if !piece.shade {
            return None;
        }
        let f = ((now - self.spark_since).max(0.0) / ELECTRIC_FRAME_TIME) as usize;
        let [from, to] = DARK_LOOP;
        Some(if f < from {
            f
        } else {
            from + (f - from + sq as usize * 5) % (to - from)
        })
    }

    /// Кадр вихря воздушной фигуры: вступление с начала партии, потом повтор (у разных фигур
    /// сдвинут по клетке).
    fn wind_frame(&self, sq: Square, piece: Piece, now: f64) -> Option<usize> {
        if !piece.wind {
            return None;
        }
        let f = ((now - self.spark_since).max(0.0) / ELECTRIC_FRAME_TIME) as usize;
        let [from, to] = WIND_LOOP;
        Some(if f < from {
            f
        } else {
            from + (f - from + sq as usize * 7) % (to - from)
        })
    }

    /// Кадр лужи ядовитой фигуры: вступление с начала партии, потом повтор (сдвинут по
    /// клетке).
    fn poison_frame(&self, sq: Square, piece: Piece, now: f64) -> Option<usize> {
        // У ядовитого зомби своей анимации лужи нет — рисуется его спрайт.
        if !piece.venom || piece.zombie {
            return None;
        }
        let f = ((now - self.spark_since).max(0.0) / ELECTRIC_FRAME_TIME) as usize;
        let [from, to] = POISON_LOOP;
        Some(if f < from {
            f
        } else {
            from + (f - from + sq as usize * 5) % (to - from)
        })
    }

    /// Кадр электричества слона: вступление со вспышкой с начала партии, потом повтор; во
    /// время своего удара — снова вспышка.
    fn spark_frame(&self, sq: Square, piece: Piece, now: f64) -> Option<usize> {
        if !piece.spark {
            return None;
        }
        let striking = self.strikes.iter().find(|s| {
            s.strike.attacker.spark
                && (s.strike.from == sq || s.moving(now) && s.strike.hit.square == sq)
                && (s.start..s.end()).contains(&now)
        });
        if let Some(s) = striking {
            // Вспышка удара — с прежней скоростью: первые кадры трёх ступеней вспышки.
            let step = ((now - s.start) / ELECTRIC_FRAME_TIME) as usize % 3;
            return Some(ELECTRIC_FLASH + ELECTRIC_SLOW * step);
        }
        let f = ((now - self.spark_since).max(0.0) / ELECTRIC_FRAME_TIME) as usize;
        let [from, to] = ELECTRIC_LOOP;
        // Повтор у разных фигур сдвинут по клетке, чтобы разряды не шли хором.
        let phase = sq as usize * 11 * ELECTRIC_SLOW;
        Some(if f < from {
            f
        } else {
            from + (f - from + phase) % (to - from)
        })
    }

    /// Фигура на клетке со своей лавой или электричеством.
    fn dressed(&self, sq: Square, piece: Piece, at: Vec2, now: f64) -> Figure {
        let lava = self.lava_frame(sq, piece, now);
        // Огненная с другой силой (rogue-like): в ожидании взрыва — кадры силы на теле с
        // трещинами, а насыщение и взрыв — лава.
        let [from, to] = LAVA_HOLD;
        let calm = lava.is_none_or(|f| (from..to).contains(&f));
        Figure {
            lava,
            spark: self.spark_frame(sq, piece, now).filter(|_| calm),
            wind: self.wind_frame(sq, piece, now).filter(|_| calm),
            poison: self.poison_frame(sq, piece, now),
            dark: self
                .dark_frame(sq, piece, now)
                .filter(|_| calm)
                .map(Dark::Aura),
            ..Figure::new(at, piece)
        }
    }

    /// Выпад фигуры с клетки `sq`, если она сейчас бьёт.
    fn strike_lunge(&self, sq: Square, now: f64) -> Vec2 {
        self.strikes
            .iter()
            .filter(|s| s.strike.from == sq)
            .map(|s| s.lunge(now))
            .fold(Vec2::ZERO, |sum, v| sum + v)
    }

    fn tick(&mut self, now: f64) {
        self.advance_animation(now, false);
        self.queue_strikes(now);
        self.advance_strikes(now);
        self.lavas.retain(|a| now < a.start + a.length());
        self.appears.retain(|&(_, start)| now < start + APPEAR_TIME);
        self.marches
            .retain(|&(_, _, start)| now < start + SLIDE_TIME);
        self.trails.retain(|&(_, _, start)| {
            now < start + POISON_TRAIL_FRAMES as f64 * ELECTRIC_FRAME_TIME
        });
        self.due_hits.retain(|&(_, at)| now < at);
        self.zaps.retain(|z| now < z.start + ZAP_TIME);
        self.craters.retain(|c| c.2.is_none_or(|end| now < end));
        self.meteor_waves
            .retain(|w| now < w.1 + METEOR_WAVE_FRAMES as f64 * WAVE_FRAME_TIME);
        for meteor in &mut self.meteors {
            let impact = meteor_impact(meteor.1);
            if !meteor.2 && now >= impact {
                meteor.2 = true;
                self.sfx
                    .play(if meteor.3 { Sound::Kill } else { Sound::Hit });
            }
        }
        self.meteors.retain(|m| {
            now < meteor_impact(m.1) + (METEOR_FRAMES - METEOR_IMPACT) as f64 * METEOR_FRAME_TIME
        });
        self.effects.retain(|e| now - e.start < EFFECT_TIME);
        self.tick_cards(now);
        if self.anim.is_none() && self.game.is_over() && self.ended_at.is_none() {
            self.ended_at = Some(now);
            self.sfx.play(Sound::Notify);
        }
        self.drive_computer(now);
        self.drive_remote(now);
    }

    // ----- Мультиплеер -----

    /// Подключение к серверу с экрана «Мультиплеер».
    pub(super) fn connect(&mut self, ctx: &egui::Context) {
        net::save_address(&self.net_address);
        self.net_status = "Подключаемся…".to_owned();
        self.net = Some(net::Link::connect(&self.net_address, ctx.clone()));
    }

    /// Закрывает соединение и выходит из сетевой партии.
    pub(super) fn leave_network(&mut self) {
        self.net = None;
        self.remote = None;
        self.net_note = None;
        self.net_queue.clear();
        self.net_pending = None;
    }

    /// События соединения: на экране подключения — состояние, в партии — ходы соперника и
    /// заметки.
    fn poll_net(&mut self) {
        let Some(link) = &self.net else {
            return;
        };
        let events: Vec<net::NetEvent> = link.rx.try_iter().collect();
        for event in events {
            match event {
                net::NetEvent::Connected => {
                    self.net_status = "Подключено. Ждём соперника…".to_owned();
                }
                net::NetEvent::Waiting => {
                    self.net_status = "Подключено. Ждём второго игрока…".to_owned();
                }
                net::NetEvent::Start(color) => {
                    // Правила выбирают белые (кто зашёл первым) и шлют их сразу, до ходов.
                    if color == Color::White {
                        let advance = self.settings.advance;
                        if let Some(link) = &self.net {
                            link.send(&net::rules_line(advance));
                        }
                        self.start_network_game(color, advance);
                    } else {
                        self.net_pending = Some(color);
                        self.net_status = "Соперник найден, ждём его правила…".to_owned();
                    }
                }
                net::NetEvent::Line(line) => {
                    if let Some(advance) = net::parse_rules(&line) {
                        if let Some(color) = self.net_pending.take() {
                            self.start_network_game(color, advance);
                        }
                    } else if let Some(version) = line.strip_prefix("VER ") {
                        if version != env!("CARGO_PKG_VERSION") {
                            self.net_note = Some(format!(
                                "У соперника версия {version}, у вас {} — партия может разойтись",
                                env!("CARGO_PKG_VERSION")
                            ));
                        }
                    } else if line == net::END_TURN || line.starts_with("M ") {
                        self.net_queue.push_back(line);
                    }
                    // Прочие строки (пульс старого сервера, незнакомое) не трогают партию.
                }
                net::NetEvent::Left => {
                    self.net_note = Some("Соперник отключился".to_owned());
                    self.net_status = "Соперник отключился".to_owned();
                    self.net = None;
                }
                net::NetEvent::Closed(reason) => {
                    if self.remote.is_some() {
                        self.net_note = Some(format!("Связь потеряна: {reason}"));
                    }
                    self.net_status = reason;
                    self.net = None;
                }
            }
            if self.net.is_none() {
                break;
            }
        }
    }

    /// Сервер посадил за доску: наш цвет — `color`. Баланс — стандартный, чтобы обе игры
    /// считали одинаково.
    fn start_network_game(&mut self, color: Color, advance: bool) {
        balance::use_default();
        self.art.sync_costs();
        self.game = Game::from_position(Position::start().without_cards().with_advance(advance));
        self.computer = None;
        self.remote = Some(color.opponent());
        self.flipped = color == Color::Black;
        self.orbit = None;
        self.net_note = None;
        self.net_queue.clear();
        self.reset_interaction();
        self.screen = Screen::Game;
        self.ended_at = None;
        if let Some(link) = &self.net {
            link.send(&net::version_line());
        }
        self.sfx.play(Sound::Notify);
    }

    /// Ходы соперника по сети — по одному, когда доиграли анимации, как у компьютера.
    fn drive_remote(&mut self, now: f64) {
        let Some(remote) = self.remote else {
            return;
        };
        if self.game.is_over() || self.game.position().side() != remote {
            return;
        }
        if self.anim.is_some() || !self.strikes.is_empty() {
            return;
        }
        let Some(line) = self.net_queue.pop_front() else {
            return;
        };
        if line == net::END_TURN {
            if self.game.can_end_turn() {
                self.end_turn(now);
            } else {
                self.net_note = Some("Партии разошлись: соперник закончил ход раньше".to_owned());
            }
        } else if let Some(mv) = net::decode_move(&line, self.game.legal_moves()) {
            self.play(mv, None, now);
        } else {
            self.net_note = Some(format!("Партии разошлись: непонятный ход «{line}»"));
        }
    }

    /// Компьютер (волны rogue-like) сам не ходит: фигуры волн шагают в конце его хода
    /// (`march`). Он ждёт, пока доиграют ход, удары (метеориты, яд) и карты игрока, и через
    /// `COMPUTER_DELAY` кончает ход.
    fn drive_computer(&mut self, now: f64) {
        let Some(computer) = self.computer else {
            return;
        };
        if self.game.is_over() || self.game.position().side() != computer {
            self.computer_since = None;
            return;
        }
        if self.anim.is_some()
            || !self.strikes.is_empty()
            || !self.due_hits.is_empty()
            || self.card_effect_running(now)
        {
            return;
        }
        let since = *self.computer_since.get_or_insert(now);
        if now - since >= COMPUTER_DELAY {
            self.computer_since = None;
            self.end_turn(now);
        }
    }

    // ----- Мышь и клавиатура -----

    fn shortcuts(&mut self, ctx: &egui::Context, now: f64) {
        let (flip, undo, fps, end, look) = ctx.input(|i| {
            (
                i.key_pressed(Key::F),
                i.modifiers.command && i.key_pressed(Key::Z),
                i.key_pressed(Key::F3),
                i.key_pressed(Key::Space) || i.key_pressed(Key::Enter),
                i.key_pressed(Key::V),
            )
        });
        // Space и Enter заканчивают ход, если клавиатура не занята полем ввода.
        // В осмотре ход не кончается: иначе легко закрыть его, разглядывая доску.
        if end && self.orbit.is_none() && self.button_active() && !ctx.wants_keyboard_input() {
            self.end_turn(now);
        }
        if look && !ctx.wants_keyboard_input() {
            self.toggle_orbit();
        }
        if flip {
            self.flip_board();
        }
        if fps {
            self.settings.show_fps = !self.settings.show_fps;
        }
        if undo {
            self.undo();
        }
    }

    fn flip_board(&mut self) {
        self.flipped = !self.flipped;
        if let Some(orbit) = &mut self.orbit {
            orbit.turn_around();
        }
    }

    fn toggle_orbit(&mut self) {
        self.drag = None;
        self.hovered = None;
        self.stats_for = None;
        if self.orbit.take().is_some() {
            return;
        }
        if self.holding_card() {
            self.cancel_card(self.now);
        }
        self.orbit = Some(orbit::Orbit::iso(self.flipped));
    }

    /// Осмотр: мышь крутит камеру, колёсико приближает, Esc возвращает плоскую доску.
    /// Щелчки по клеткам в этом режиме не ходят.
    fn orbit_input(&mut self, ui: &egui::Ui, board: &BoardGeom) {
        self.drag = None;
        self.card_hover = None;
        self.button_hover = false;
        self.button_down = false;
        self.reserve_hover = None;
        self.shop_hover = None;
        self.power_hover = None;
        self.crest_hover = None;
        self.stats_for = None;
        self.deck_hover = None;
        let (down, delta, esc, scroll, command, pointer) = ui.input(|i| {
            (
                i.pointer.primary_down() || i.pointer.secondary_down(),
                i.pointer.delta(),
                i.key_pressed(Key::Escape),
                i.raw_scroll_delta.y,
                i.modifiers.command,
                i.pointer.latest_pos(),
            )
        });
        if esc {
            self.orbit = None;
            self.hovered = None;
            return;
        }
        let Some(orbit) = &mut self.orbit else {
            return;
        };
        // Ctrl+колёсико по-прежнему меняет масштаб всего поля, а не камеру.
        if scroll != 0.0 && !command {
            orbit.zoom(scroll);
        }
        if down {
            orbit.drag(delta);
        }
        let orbit = *orbit;
        self.hovered = pointer
            .filter(|p| board.area.contains(*p))
            .and_then(|p| orbit.square_at(p, board.rect()));
    }

    fn orbit_marks(&self) -> orbit::Marks {
        let over = self.game.is_over();
        let pos = self.game.position();
        let last = self
            .game
            .history()
            .iter()
            .rev()
            .take_while(|r| r.turn_end().is_none_or(|t| t.planning.is_none()))
            .find_map(|r| r.outcome());
        let side = pos.side();
        let king = if over { None } else { pos.king_square(side) };
        orbit::Marks {
            hovered: self.hovered,
            selected: self.selected,
            last_from: last.map(|o| o.mv.from),
            last_to: last.map(|o| o.mv.to),
            check: king.filter(|_| pos.in_check(side)),
            threat: king.filter(|_| !pos.in_check(side) && pos.king_danger(side) > 0),
        }
    }

    fn paint_orbit(&self, painter: &Painter, board: &BoardGeom, tex: &Textures) {
        let Some(orbit) = self.orbit else {
            return;
        };
        let marks = self.orbit_marks();
        orbit::paint(
            painter,
            board.rect(),
            tex,
            orbit,
            self.game.position(),
            marks,
            self.flipped,
        );
    }

    fn paint_orbit_hint(&self, painter: &Painter, board: &BoardGeom) {
        if self.orbit.is_none() {
            return;
        }
        let text = "Мышь — крутить · колёсико — ближе · Esc — назад";
        let galley =
            painter.layout_no_wrap(text.to_string(), FontId::proportional(13.0), theme::TEXT_BRIGHT);
        let rect = Rect::from_center_size(
            pos2(board.rect().center().x, board.rect().top() + 16.0),
            galley.size() + vec2(16.0, 8.0),
        );
        painter.rect_filled(rect, CornerRadius::same(4), rgba(0, 0, 0, 0.62));
        painter.galley(rect.min + vec2(8.0, 4.0), galley, theme::TEXT_BRIGHT);
    }

    fn handle_input(&mut self, ui: &egui::Ui, board: &BoardGeom, now: f64) {
        if self.orbit.is_some() {
            self.orbit_input(ui, board);
            return;
        }
        let (pressed, released, cancel, pointer) = ui.input(|i| {
            (
                i.pointer.primary_pressed(),
                i.pointer.primary_released(),
                i.pointer.secondary_pressed() || i.key_pressed(Key::Escape),
                i.pointer.latest_pos(),
            )
        });
        // Взятая карта тянется за курсором; отпущенная над рукой возвращается, в
        // другом месте — разыгрывается. Правая кнопка или Esc возвращают её в руку.
        let over_board = ui.rect_contains_pointer(board.area);
        self.crest_hover = pointer
            .filter(|_| over_board)
            .and_then(|p| self.crest_under(board, p));
        if self.holding_card() {
            let at = pointer.map(|p| board.to_art(p));
            self.carry_card(at, now);
            if released {
                self.drop_card(at, now);
            } else if cancel {
                self.cancel_card(now);
            }
            self.hovered = None;
            self.card_hover = None;
            self.stats_for = None;
            self.deck_hover = None;
            self.button_hover = false;

            self.reserve_hover = None;
            return;
        }
        // Rogue-like: пока открыт магазин, щелчки — только по нему; пока доигрывают удары,
        // магазина ещё нет, и щелчки не нужны вовсе. Свёрнутый магазин (с 0.126) щелчков не
        // ловит, кроме кнопки «Магазин»: доска и карты — как в ходу.
        self.shop_hover = None;
        self.unfold_hover = false;
        self.card_menu_hover = None;
        if !self.game.position().shop_open() {
            self.shop_folded = false;
            self.fold_for_card = false;
            self.card_menu = None;
        }
        // Свёрнут ради цели карты — карту сыграли или отменили: магазин снова виден.
        if self.fold_for_card && self.power_pick.is_none() {
            self.shop_folded = false;
            self.fold_for_card = false;
        }
        if self.game.position().shop_open() && !self.shop_ready() {
            self.hovered = None;
            self.stats_for = None;
            self.reserve_hover = None;
            self.button_hover = false;
            self.drag = None;
            return;
        }
        if self.game.position().shop_open() && self.human_to_move() && self.shop_folded {
            self.unfold_hover = pointer
                .is_some_and(|p| over_board && table::unfold_contains(board.to_art(p).to_pos2()));
            if pressed && self.unfold_hover {
                self.shop_folded = false;
                self.fold_for_card = false;
                self.power_pick = None;
                self.mirror_from = None;
                return;
            }
        }
        if self.game.position().shop_open() && self.human_to_move() && !self.shop_folded {
            self.shop_hover = pointer
                .filter(|_| over_board)
                .and_then(|p| self.shop_under(board, p));
            self.card_menu_hover = pointer
                .filter(|_| over_board)
                .and_then(|p| self.card_menu_under(board, p));
            // Фигуру своего запаса можно взять и перенести в зону продажи.
            self.reserve_hover = pointer
                .filter(|_| over_board)
                .and_then(|p| self.reserve_under(board, p))
                .filter(|&(seat, stock)| self.reserve_sellable(seat, stock));
            if let (Some(sell), Some(p)) = (&mut self.sell_drag, pointer) {
                sell.1 = p;
            }
            self.power_hover = pointer
                .filter(|_| over_board)
                .and_then(|p| self.hand_under(board, p));
            let sell_spot = self.crest_sell_spot(board);
            self.crest_sell_hover = pointer
                .zip(sell_spot)
                .is_some_and(|(p, (_, rect))| rect.contains(board.to_art(p).to_pos2()));
            self.hovered = None;
            self.stats_for = None;
            self.button_hover = false;
            self.card_hover = None;
            self.deck_hover = None;
            self.drag = None;
            if released && let Some((stock, _)) = self.sell_drag.take() {
                if self.shop_hover == Some(ShopSpot::Sell) && self.game.sell(stock) {
                    self.sfx.play(Sound::Kill);
                }
                return;
            }
            if pressed && self.crest_sell_hover {
                if let Some((index, _)) = sell_spot
                    && self.game.sell_crest(index)
                {
                    self.sfx.play(Sound::Kill);
                }
                self.crest_sell = None;
                return;
            }
            // Меню карты (с 0.126): «Сыграть» — как щелчок по карте в ходу (карте нужна цель —
            // магазин сворачивается, чтобы была видна доска), «Продать» — за `card_sell_price`.
            if pressed && let (Some(item), Some(slot)) = (self.card_menu_hover, self.card_menu) {
                self.card_menu = None;
                if !self.card_menu_usable(item) {
                    return;
                }
                match item {
                    CardMenu::Play => {
                        self.click_card(slot);
                        if self.power_pick.is_some() {
                            self.shop_folded = true;
                            self.fold_for_card = true;
                        }
                    }
                    CardMenu::Sell => {
                        if self.game.sell_card(slot) {
                            self.sfx.play(Sound::Kill);
                        }
                    }
                }
                return;
            }
            if pressed {
                // Щелчок по своему гербу в колонке выбирает его на продажу, мимо — снимает выбор.
                let crest = pointer.and_then(|p| self.column_crest_under(board, p));
                self.crest_sell = crest.filter(|&c| self.crest_sell != Some(c));
                if crest.is_some() {
                    return;
                }
            }
            if pressed && let (Some((_, stock)), Some(p)) = (self.reserve_hover, pointer) {
                self.sell_drag = Some((stock, p));
                return;
            }
            if pressed && let Some(slot) = self.power_hover {
                self.power_pick = None;
                self.card_menu = (self.card_menu != Some(slot)).then_some(slot);
                return;
            }
            if pressed {
                self.card_menu = None;
            }
            if pressed && let Some(spot) = self.shop_hover.filter(|&s| self.shop_usable(s)) {
                let done = match spot {
                    ShopSpot::Fold => {
                        self.shop_folded = true;
                        self.crest_sell = None;
                        return;
                    }
                    ShopSpot::Reroll => self.game.reroll(),
                    ShopSpot::Leave => self.game.leave_shop(),
                    ShopSpot::Chest(slot) => self.game.open_chest(slot),
                    ShopSpot::Pick(i) => self.game.take_from_chest(i),
                    ShopSpot::Sell => false,
                };
                if done {
                    self.sfx.play(Sound::Move);
                }
            }
            return;
        }
        self.sell_drag = None;
        self.crest_sell = None;
        self.crest_sell_hover = false;
        // Кнопка «Конец хода» срабатывает, когда её отпускают, не уведя курсор.
        self.button_hover = pointer.is_some_and(|p| over_board && self.over_button(board, p));

        self.reserve_hover = pointer
            .filter(|_| over_board)
            .and_then(|p| self.reserve_under(board, p));
        if released && std::mem::take(&mut self.button_down) && self.button_hover {
            self.end_turn(now);
        }
        let carried = self.drag.as_ref().filter(|d| d.active).map(|d| d.grab);
        // Над колодой — табличка с числом карт в ней.
        let deck = pointer
            .filter(|_| over_board && carried.is_none() && self.promotion.is_none())
            .and_then(|p| self.deck_under(board, p));
        if self.deck_hover.map(|(seat, _)| seat) != deck {
            self.deck_hover = deck.map(|seat| (seat, now));
        }
        // Карты лежат поверх стола: над картой доска курсор не ловит.
        let card = pointer
            .filter(|_| {
                over_board
                    && carried.is_none()
                    && self.promotion.is_none()
                    && !self.aiming_card()
                    && !self.button_hover
            })
            .and_then(|p| self.card_under(board, p));
        if card != self.card_hover {
            self.card_hover_since = now;
        }
        self.card_hover = card;
        self.hovered = pointer
            .filter(|_| over_board && self.card_hover.is_none() && !self.button_hover)
            .and_then(|p| match carried {
                // Перетаскиваемая фигура встаёт на клетку под своим основанием.
                Some(grab) => board.square_at(p + grab),
                None => self.square_under(board, p),
            });
        // У цели карты таблички закрыла бы стрелка.
        let shown = self
            .hovered
            .filter(|&sq| self.game.position().piece(sq).is_some() && !self.aim_pickable(sq));
        if self.stats_for.map(|(sq, _)| sq) != shown {
            self.stats_for = shown.map(|sq| (sq, now));
        }

        if self.promotion.is_some() {
            if cancel {
                self.promotion = None;
            } else if pressed {
                self.choose_promotion(now);
            }
            return;
        }
        // Карта ждёт цель: щелчок по своей фигуре применяет её, мимо — возвращает в руку.
        if self.aiming_card() {
            if cancel {
                self.cancel_aim();
            } else if pressed {
                match self.hovered.filter(|&sq| self.aim_pickable(sq)) {
                    Some(target) => self.aim_at(target, now),
                    None => self.cancel_aim(),
                }
            }
            self.drag = None;
            return;
        }
        if cancel {
            self.drop_pick = None;
            self.power_pick = None;
            self.mirror_from = None;
        }
        // Карта в руке: щелчок играет её сразу или выбирает (подсвечены свои фигуры, на которые
        // её можно сыграть), повторный — снимает выбор.
        self.power_hover = pointer
            .filter(|_| over_board)
            .and_then(|p| self.hand_under(board, p));
        if pressed && let Some(i) = self.power_hover {
            self.click_card(i);
            self.drop_pick = None;
            self.selected = None;
            self.drag = None;
            return;
        }
        if pressed && let Some((seat, stock)) = self.reserve_hover {
            self.power_pick = None;
            self.mirror_from = None;
            self.drop_pick = if self.reserve_usable(seat, stock) && self.drop_pick != Some(stock) {
                Some(stock)
            } else {
                None
            };
            self.selected = None;
            self.drag = None;
            return;
        }

        if pressed && self.button_hover {
            self.button_down = self.button_active();
            self.drag = None;
            return;
        }
        if let Some((seat, k)) = self.card_hover {
            if pressed && let Some(p) = pointer {
                self.drag = None;
                self.grab_card(seat, k, board.to_art(p), now);
            }
            if released {
                self.drag = None;
            }
            return;
        }
        if !self.human_to_move() {
            self.drag = None;
            return;
        }
        if pressed
            && let Some(p) = pointer
            && let Some(sq) = self.hovered
        {
            self.press(sq, p, board, now);
        }
        if let Some(drag) = &mut self.drag
            && let Some(p) = pointer
        {
            drag.pointer = p;
            drag.active |= drag.press.distance(p) > 4.0;
        }
        if released {
            self.release(pointer, board, now);
        }
    }

    fn press(&mut self, sq: Square, pointer: Pos2, board: &BoardGeom, now: f64) {
        // Выбрана карта: щелчок по подсвеченной своей фигуре играет её (у зеркала первый щелчок
        // выбирает фигуру, которая станет копией), мимо — отменяет.
        if let Some(slot) = self.power_pick.take()
            && let Some(card) = self.picked_card_at(slot)
            && self.card_squares(card).contains(&sq)
        {
            self.selected = None;
            self.drag = None;
            if card.picks() == 2 && self.mirror_from.is_none() {
                self.mirror_from = Some(sq);
                self.power_pick = Some(slot);
                return;
            }
            let targets: Vec<Square> = self.mirror_from.take().into_iter().chain([sq]).collect();
            if self.game.use_card(slot, &targets) {
                self.sfx.play(Sound::Notify);
            }
            return;
        }
        self.mirror_from = None;
        // Выбрана фигура запаса: щелчок по подсвеченной клетке выставляет её, мимо — отменяет.
        if let Some(stock) = self.drop_pick.take() {
            let drop = self
                .game
                .legal_moves()
                .iter()
                .copied()
                .find(|m| m.kind == stock.drop_kind() && m.to == sq);
            if let Some(mv) = drop {
                self.play(mv, None, now);
                return;
            }
        }
        if let Some(from) = self.selected
            && from != sq
            && self.has_move(from, sq)
        {
            self.attempt(from, sq, None, now);
            return;
        }
        let pos = self.game.position();
        // Фигурой ходят раз за ход (в rogue-like — несколько ходов разными фигурами);
        // замёрзшая фигура не ходит.
        if pos.can_move(sq)
            && pos
                .piece(sq)
                .is_some_and(|p| p.color == pos.side() && !p.is_frozen())
        {
            let was_selected = self.selected == Some(sq);
            self.selected = Some(sq);
            self.drag = Some(Drag {
                from: sq,
                press: pointer,
                pointer,
                grab: board.ground(sq) - pointer,
                active: false,
                was_selected,
            });
        } else {
            self.selected = None;
        }
    }

    fn release(&mut self, pointer: Option<Pos2>, board: &BoardGeom, now: f64) {
        let Some(drag) = self.drag.take() else {
            return;
        };
        if drag.active {
            if let Some(base) = pointer.map(|p| p + drag.grab)
                && let Some(to) = board.square_at(base)
                && to != drag.from
                && self.has_move(drag.from, to)
            {
                self.attempt(drag.from, to, Some(board.to_board(base)), now);
            }
        } else if drag.was_selected {
            // Второй щелчок по выбранной фигуре снимает выделение.
            self.selected = None;
        }
    }

    /// Клетка под курсором. Фигуры выше своих клеток и закрывают дальние клетки, поэтому
    /// сначала проверяются они — от ближних к дальним (по хитбоксам `Art::hits`: силуэт чуть
    /// меньше видимого, у короля — без верха), потом ромбы клеток.
    fn square_under(&self, board: &BoardGeom, pointer: Pos2) -> Option<Square> {
        // Клетка-цель выбранной фигуры или фигуры запаса важнее фигур, которые её загораживают
        // (с 0.57, просьба пользователя): ходят только на пустые клетки, ошибки не будет.
        if self.promotion.is_none()
            && self.human_to_move()
            && let Some(sq) = board.square_at(pointer)
            && self.is_target(sq)
        {
            return Some(sq);
        }
        let mut figures: Vec<(Square, Piece)> = match &self.promotion {
            Some(pending) => {
                let color = self.game.position().side();
                self.promotion_choices(pending)
                    .into_iter()
                    .map(|(sq, kind)| (sq, Piece::new(kind, color)))
                    .collect()
            }
            None => self.game.position().pieces().collect(),
        };
        figures.sort_by(|a, b| {
            board
                .depth(center(b.0))
                .total_cmp(&board.depth(center(a.0)))
        });
        figures
            .into_iter()
            .find(|&(sq, piece)| {
                let at = (pointer - board.sprite_rect(board.ground(sq), 1.0).min) / board.scale;
                self.art.hits(piece.color, piece.kind, at)
            })
            .map(|(sq, _)| sq)
            .or_else(|| board.square_at(pointer))
    }

    fn attempt(&mut self, from: Square, to: Square, drop: Option<Vec2>, now: f64) {
        let moves: Vec<Move> = self
            .game
            .legal_moves()
            .iter()
            .copied()
            .filter(|m| m.from == from && m.to == to && !m.is_drop())
            .collect();
        match moves.as_slice() {
            [] => {}
            [mv] => self.play(*mv, drop, now),
            _ => {
                self.promotion = Some(PendingPromotion { from, to, drop });
                self.selected = None;
            }
        }
    }

    fn promotion_choices(&self, pending: &PendingPromotion) -> Vec<(Square, Kind)> {
        let color = self.game.position().side();
        let file = game::file_of(pending.to);
        let (first_rank, step) = if color == Color::White {
            (7, -1)
        } else {
            (0, 1)
        };
        Kind::PROMOTIONS
            .iter()
            .enumerate()
            .filter_map(|(i, &kind)| Some((game::square(file, first_rank + step * i as i8)?, kind)))
            .collect()
    }

    fn choose_promotion(&mut self, now: f64) {
        let Some(pending) = self.promotion.take() else {
            return;
        };
        let choice = self
            .promotion_choices(&pending)
            .into_iter()
            .find(|&(sq, _)| Some(sq) == self.hovered)
            .map(|(_, kind)| kind);
        // Щелчок мимо отменяет ход, как на lichess.
        if let Some(kind) = choice
            && let Some(&mv) =
                self.game.legal_moves().iter().find(|m| {
                    m.from == pending.from && m.to == pending.to && m.promotion == Some(kind)
                })
        {
            self.play(mv, pending.drop, now);
        }
    }

    // ----- Доска -----

    fn board_area(&mut self, ui: &mut egui::Ui, now: f64) {
        let avail = ui.available_rect_before_wrap();
        paint_backdrop(ui.painter(), ui.clip_rect());
        let ppp = ui.ctx().pixels_per_point();
        // Карт нет (с 0.40): вписываются доска, кнопка и запас (`VIEW_MIN`…`VIEW_MAX`), а не
        // стол с руками и колодами. Ctrl/⌘ + колесо над доской — свой масштаб (`Settings::zoom`).
        // В rogue-like слева от доски ещё колонка гербов (с 0.114, на месте бывшей левой
        // панели) — `ROGUE_VIEW_MIN`.
        let rogue = self.game.position().rogue().is_some();
        let view_min = if rogue { ROGUE_VIEW_MIN } else { VIEW_MIN };
        let view_max = VIEW_MAX;
        let need = view_max - view_min;
        let wheel = ui.input(|i| i.modifiers.command.then(|| i.raw_scroll_delta.y));
        if let Some(dy) = wheel.filter(|dy| *dy != 0.0)
            && ui.rect_contains_pointer(avail)
        {
            let step = if dy > 0.0 { 10 } else { -10 };
            self.settings.zoom = self
                .settings
                .zoom
                .saturating_add_signed(step)
                .clamp(ZOOM_MIN, ZOOM_MAX);
        }
        // Текстуры зависят только от размера окна. Масштаб из настроек их не меняет:
        // иначе каждый щелчок колёсика заново заливает все полосы анимаций, и игра замирает.
        let fit_at_100 = (avail.width() / need.x).min(avail.height() / need.y) * ppp;
        let fit = fit_at_100 * f32::from(self.settings.zoom) / 100.0;
        // Пиксель арта — целое число физических пикселей, если доска от этого мельчает не
        // больше чем на 8% (сетка пикселей ровная); иначе — дробный масштаб на всё место.
        let whole = (fit + 0.01 * fit).floor();
        let physical = if whole >= 1.0 && whole >= 0.92 * fit {
            whole
        } else {
            fit.max(0.25)
        };
        let scale = physical / ppp;
        let corner = avail.center() - need * scale / 2.0 - view_min * scale;
        let origin = pos2(
            (corner.x * ppp).round() / ppp,
            (corner.y * ppp).round() / ppp,
        );
        ui.allocate_rect(avail, Sense::click_and_drag());
        let board = BoardGeom {
            area: avail,
            origin,
            scale,
            texture_scale: texture_scale_for(fit_at_100),
            ppp,
            flipped: self.flipped,
        };

        self.handle_input(ui, &board, now);

        let textures = self.art.textures(ui.ctx(), board.texture_scale);
        self.paint_board(ui.painter(), &board, &textures, now);
        self.paint_orbit_hint(ui.painter(), &board);
        let top = ui.ctx().layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("board-top"),
        ));
        let pointer = ui
            .ctx()
            .pointer_hover_pos()
            .filter(|_| ui.rect_contains_pointer(board.area));
        self.paint_dragged(&top, &board, &textures);
        self.paint_aim(&top, &board, &textures, pointer);
        self.paint_held(&top, &board, &textures);
        self.pointer = self.pointer_kind(&board, pointer);
        // Курсор — в ближайшем целом масштабе доски: дробный размыл бы стрелку.
        self.cursor_scale = physical.round().max(1.0) as u32;
        if self.settings.show_fps {
            self.paint_fps(ui.ctx(), avail.min);
        }
    }

    fn paint_board(&self, painter: &Painter, board: &BoardGeom, tex: &Textures, now: f64) {
        if self.orbit.is_some() {
            self.paint_orbit(painter, board, tex);
            self.paint_table(painter, board, tex, now);
            self.paint_deck_count(painter, board, tex, now);
            self.paint_banner(painter, board, now);
            return;
        }
        let pos = self.game.position();
        let anim = self.anim.as_ref();

        painter.image(tex.board(board.flipped), board.rect(), UV, Color32::WHITE);
        // Снег заморозки и ледяных зон лежит на клетках под подсветками.
        self.paint_snow(painter, board, tex, now);
        let mark = |sq: Square, marker: Marker, tint: Color32| {
            painter.image(tex.marker(marker), board.tile_rect(sq), UV, tint);
        };

        // Барьеры сторон — клетки, куда соперник не войдёт, золотистой заливкой.
        for color in [Color::White, Color::Black] {
            let wall = pos.barrier(color);
            for sq in (0..64).filter(|&sq| wall & 1 << sq != 0) {
                mark(sq, Marker::Fill, rgba(255, 196, 64, 0.28));
            }
        }

        // Последний ход: откуда пошла фигура и где встала.
        // Rogue-like: фигуры вернулись к расстановке — прежний последний ход не показывается.
        let last_move = self
            .game
            .history()
            .iter()
            .rev()
            .take_while(|r| r.turn_end().is_none_or(|t| t.planning.is_none()))
            .find_map(|r| r.outcome());
        if let Some(o) = last_move {
            mark(o.mv.from, Marker::LastMove, Color32::WHITE);
            mark(o.mv.to, Marker::LastMove, Color32::WHITE);
        }

        for effect in &self.effects {
            let age = now - effect.start;
            if effect.wounds() && (0.0..FLASH_TIME).contains(&age) {
                let k = 1.0 - (age / FLASH_TIME) as f32;
                mark(effect.square, Marker::Fill, rgba(255, 48, 24, 0.55 * k * k));
            }
        }

        // Шах (короля могут убить) — красным; просто угроза — оранжевым.
        let side = pos.side();
        if !self.game.is_over()
            && let Some(king) = pos.king_square(side)
        {
            if pos.in_check(side) {
                mark(king, Marker::Check, Color32::WHITE);
            } else if pos.king_danger(side) > 0 {
                mark(king, Marker::Threat, Color32::WHITE);
            }
        }

        if let Some(sq) = self.selected {
            mark(sq, Marker::Selected, Color32::WHITE);
        }
        let target_tint = target_tint(now);

        // Карта ждёт цель: свои фигуры, на которые её можно сыграть, а у карты на поле —
        // клетки, на которые она ляжет вокруг клетки под курсором.
        match self.aim_area() {
            Some(area) => {
                for sq in self
                    .hovered
                    .map(|aim| area.squares(aim))
                    .unwrap_or_default()
                {
                    mark(sq, Marker::Ice, Color32::WHITE);
                }
            }
            None => {
                // Чужие фигуры (ледяной снаряд и взрыв) — как цели удара.
                for &sq in self.aim_targets() {
                    let enemy = pos.piece(sq).is_some_and(|p| p.color != side);
                    let marker = if enemy { Marker::Attack } else { Marker::Move };
                    mark(sq, marker, Color32::WHITE);
                }
            }
        }

        // Куда выставить фигуру из запаса.
        if let Some(stock) = self.drop_pick
            && self.human_to_move()
        {
            for mv in self.game.legal_moves() {
                if mv.kind == stock.drop_kind() {
                    mark(mv.to, Marker::Move, Color32::WHITE);
                    if self.hovered == Some(mv.to) {
                        mark(mv.to, Marker::Target, target_tint);
                    }
                }
            }
        }

        // На какие свои фигуры сыграть выбранную карту; первая фигура зеркала — в рамке.
        if let Some(card) = self.picked_card()
            && self.human_to_move()
        {
            if let Some(from) = self.mirror_from {
                mark(from, Marker::Target, Color32::WHITE);
            }
            for sq in self.card_squares(card) {
                mark(sq, Marker::Move, Color32::WHITE);
                if self.hovered == Some(sq) {
                    mark(sq, Marker::Target, target_tint);
                }
            }
        }

        // Ходы выбранной фигуры — синим, клетка под курсором — в рамке.
        if let Some(from) = self.selected
            && self.human_to_move()
        {
            let mut marked = [false; 64];
            for mv in self
                .game
                .legal_moves()
                .iter()
                .filter(|m| m.from == from && !m.is_drop())
            {
                if std::mem::replace(&mut marked[mv.to as usize], true) {
                    continue;
                }
                mark(mv.to, Marker::Move, Color32::WHITE);
                if self.hovered == Some(mv.to) {
                    mark(mv.to, Marker::Target, target_tint);
                }
            }
        }

        let mut figures = Vec::new();
        // Фигура на своей клетке — со льдом, если заморожена.
        let standing = |sq: Square, piece: Piece| Figure {
            ice: self.ice(sq, piece, now),
            ..self.dressed(sq, piece, center(sq), now)
        };
        // Убитая ударом (и молнией) фигура: до касания стоит на месте, после — растворяется.
        let mut struck: Vec<Square> = self
            .strikes
            .iter()
            .flat_map(|s| s.strike.hits().map(|h| h.square))
            .collect();
        struck.sort_unstable();
        struck.dedup();
        for sq in struck {
            let killer_coming = self
                .strikes
                .iter()
                .any(|s| s.moving(now) && s.strike.hit.square == sq);
            // Походившую фигуру, которую перехватит конь, пока везёт ход. На клетку убитой
            // могла шагнуть фигура волны (или вернуться к расстановке фигура игрока) — она
            // ещё в пути, а убитая до касания стоит.
            let taken = self.march_origin(sq) != sq;
            if (pos.piece(sq).is_none() || taken || killer_coming)
                && !anim.is_some_and(|a| a.hides(sq))
                && let Some(victim) = self.struck_piece(sq, now)
            {
                figures.push(Figure {
                    ice: self.ice(sq, victim, now),
                    ..self.dressed(sq, victim, center(sq) + self.strike_lunge(sq, now), now)
                });
            }
        }
        // Убийца идёт на клетку цели.
        // До своего хода на клетку цели он делает выпады остальных своих ударов.
        for s in self.strikes.iter().filter(|s| s.moving(now)) {
            figures.push(self.dressed(
                s.strike.hit.square,
                s.strike.attacker,
                s.killer_at(now) + self.strike_lunge(s.strike.from, now),
                now,
            ));
        }
        for effect in &self.effects {
            let age = now - effect.start;
            if effect.killed
                && let Some(victim) = effect.victim
                && (0.0..FADE_TIME).contains(&age)
            {
                let k = (age / FADE_TIME) as f32;
                figures.push(Figure {
                    alpha: 1.0 - k,
                    grow: 1.0 + 0.25 * k,
                    ..standing(effect.square, victim)
                });
            }
        }

        // Погибшая от яда или метеорита стоит, пока урон не показался.
        for (hit, _) in self.due_hits.iter().filter(|(h, _)| h.killed) {
            figures.push(standing(hit.square, hit.target));
        }
        // Пока огненная волна не дошла, задетые фигуры — как до неё, погибшие — на месте.
        let victims = self.wave_victims();
        for hit in victims.iter().filter(|h| h.killed) {
            figures.push(standing(hit.square, hit.target));
        }
        // Фигуры обмена рисует портал.
        figures.extend(self.portal_figures(now));

        let dragged = self.drag.as_ref().filter(|d| d.active).map(|d| d.from);
        for (sq, mut piece) in pos.pieces() {
            if anim.is_some_and(|a| a.hides(sq)) || self.portal_hides(sq) {
                continue;
            }
            // Убийцу, который ещё идёт на эту клетку, рисует удар.
            if self
                .strikes
                .iter()
                .any(|s| s.moving(now) && s.strike.hit.square == sq)
            {
                continue;
            }
            if let Some(hit) = victims.iter().find(|h| h.square == sq) {
                piece = hit.target;
            }
            if let Some((hit, _)) = self.due_hits.iter().find(|(h, _)| h.square == sq) {
                piece = hit.target;
            }
            // Пока удар не долетел, у цели прежнее здоровье. Пришедшая на клетку цели фигура
            // (шаг волны, возвращение к расстановке) — не цель.
            if self.march_origin(sq) == sq
                && let Some(before) = self.struck_piece(sq, now)
            {
                piece = before;
            }
            // Бьющая фигура делает выпад к цели.
            let shift = self.strike_lunge(self.march_origin(sq), now) + self.march_shift(sq, now);
            // Исцеляющий слон — с крыльями и ореолом.
            let bless = (piece.kind == Kind::Bishop)
                .then(|| self.bless_frame(sq, piece.color, now))
                .flatten();
            figures.push(Figure {
                shake: self.shake(sq, now),
                alpha: (if dragged == Some(sq) { 0.3 } else { 1.0 }) * self.appear_alpha(sq, now),
                bless,
                ice: self.ice(sq, piece, now),
                ..self.dressed(sq, piece, center(sq) + shift, now)
            });
        }

        // Призрак фигуры на клетке-цели под курсором — куда она встанет.
        if dragged.is_none()
            && let Some((sq, piece)) = self.ghost()
        {
            figures.push(Figure {
                alpha: GHOST_ALPHA,
                shadow: false,
                ..self.dressed(sq, piece, center(sq), now)
            });
        }

        if let Some(a) = anim {
            let to = a.outcome.mv.to;
            if a.step {
                // Теневой шаг: исчезновение на исходной клетке, с кадра `STEP_IN_AT` — появление
                // на целевой; теней нет.
                let f = ((now - a.start).max(0.0) / STEP_FRAME_TIME) as usize;
                if f < STEP_OUT {
                    figures.push(Figure {
                        dark: Some(Dark::Out(f)),
                        shadow: false,
                        ..Figure::new(center(a.outcome.mv.from), a.outcome.piece)
                    });
                }
                if let Some(piece) = pos.piece(to)
                    && f >= STEP_IN_AT
                {
                    figures.push(Figure {
                        dark: Some(Dark::In((f - STEP_IN_AT).min(STEP_IN - 1))),
                        shadow: false,
                        ..Figure::new(center(to), piece)
                    });
                }
            } else if let Some(piece) = self.struck_piece(to, now).or(pos.piece(to)) {
                // Перехваченная конём — прежняя, пока удар не дошёл.
                figures.push(Figure {
                    shake: self.shake(to, now),
                    ice: self.ice(to, piece, now),
                    ..self.dressed(to, piece, a.piece_at(now), now)
                });
            }
            if let Some((sq, at)) = a.rook_at(now)
                && let Some(rook) = pos.piece(sq)
            {
                figures.push(Figure {
                    ice: self.ice(sq, rook, now),
                    ..Figure::new(at, rook)
                });
            }
        }
        // Огненная волна и порталы обмена: что позади — под фигурами, остальное — над ними.
        self.paint_trails(painter, board, tex, now);
        self.paint_graves(painter, board, tex, now);
        self.paint_craters(painter, board, tex, false, now);
        self.paint_meteor_waves(painter, board, tex, false, now);
        self.paint_waves(painter, board, tex, false, now);
        self.paint_portals(painter, board, tex, false, now);
        self.paint_blesses(painter, board, tex, false, now);
        self.paint_figures(painter, board, tex, &mut figures);
        self.paint_craters(painter, board, tex, true, now);
        self.paint_meteor_waves(painter, board, tex, true, now);
        self.paint_skulls(painter, board, tex, &figures);
        self.paint_meteors(painter, board, tex, now);
        self.paint_zaps(painter, board, now);
        self.paint_freeze_over(painter, board, tex, now);
        self.paint_blesses(painter, board, tex, true, now);
        self.paint_portals(painter, board, tex, true, now);
        self.paint_waves(painter, board, tex, true, now);

        // Атака и здоровье фигуры под курсором — по бокам от неё.
        if let Some((sq, since)) = self.stats_for
            && dragged != Some(sq)
            && !anim.is_some_and(|a| a.hides(sq))
            && let Some(piece) = pos.piece(sq)
        {
            self.paint_stats(painter, board, tex, sq, piece, now - since);
        }

        for effect in &self.effects {
            let piece = effect.victim.or_else(|| pos.piece(effect.square));
            let anchor = self.label_point(board, effect.square, piece);
            self.paint_damage_number(painter, board, tex, anchor, effect, now);
        }

        self.paint_table(painter, board, tex, now);
        self.paint_deck_count(painter, board, tex, now);

        if let Some(pending) = &self.promotion {
            self.paint_promotion(painter, board, tex, pending);
        }
        self.paint_banner(painter, board, now);
    }

    /// Следы ядовитых фигур, шагнувших вперёд: капля между клетками, под фигурами.
    fn paint_trails(&self, painter: &Painter, board: &BoardGeom, tex: &Textures, now: f64) {
        for &(from, to, start) in &self.trails {
            // След тянется за фигурой, когда она шагает.
            if now < start {
                continue;
            }
            let frame = ((now - start) / ELECTRIC_FRAME_TIME) as usize;
            if frame >= POISON_TRAIL_FRAMES {
                continue;
            }
            let (a, b) = (board.ground(from), board.ground(to));
            let d = b - a;
            let direction = match (d.x >= 0.0, d.y <= 0.0) {
                (true, true) => 0,
                (false, false) => 1,
                (false, true) => 2,
                (true, false) => 3,
            };
            let Some(sprite) = tex.ui.poison_trail(direction, frame) else {
                continue;
            };
            let mid = a + d / 2.0;
            let origin = board.snap(mid - POISON_TRAIL_CENTER * board.scale);
            let rect =
                Rect::from_min_size(origin + sprite.at * board.scale, sprite.size * board.scale);
            painter.image(sprite.texture, rect, UV, Color32::WHITE);
        }
    }

    /// Разряды молний: ломаная от фигуры к фигуре в пикселях арта — голубое свечение в три
    /// пикселя и белое ядро; ломаная перескакивает каждые `ZAP_FLICKER`, к концу гаснет.
    fn paint_zaps(&self, painter: &Painter, board: &BoardGeom, now: f64) {
        let s = board.scale;
        for zap in &self.zaps {
            let t = now - zap.start;
            if !(0.0..ZAP_TIME).contains(&t) {
                continue;
            }
            let fade = (1.0 - t / ZAP_TIME) as f32;
            let a = board.to_art(board.ground(zap.from)) - vec2(0.0, zap.lift[0]);
            let b = board.to_art(board.ground(zap.to)) - vec2(0.0, zap.lift[1]);
            let d = b - a;
            let n = ((d.length() / 7.0).ceil() as usize).max(2);
            let normal = vec2(-d.y, d.x).normalized();
            // Случайность ломаной — клетки и номер вспышки.
            let mut state = (u64::from(zap.from) << 8 | u64::from(zap.to))
                ^ ((t / ZAP_FLICKER) as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15)
                | 1;
            let mut next = move || {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                (state % 1000) as f32 / 1000.0
            };
            let points: Vec<Vec2> = (0..=n)
                .map(|i| {
                    let f = i as f32 / n as f32;
                    let off = if i == 0 || i == n {
                        0.0
                    } else {
                        (next() - 0.5) * 7.0
                    };
                    a + d * f + normal * off
                })
                .collect();
            let mut pixels = Vec::new();
            for seg in points.windows(2) {
                let steps = (seg[1] - seg[0]).length().ceil().max(1.0) as usize;
                for k in 0..=steps {
                    let p = seg[0] + (seg[1] - seg[0]) * (k as f32 / steps as f32);
                    pixels.push(vec2(p.x.floor(), p.y.floor()));
                }
            }
            for (size, color) in [
                (3.0, ZAP_GLOW.gamma_multiply(0.55 * fade)),
                (1.0, ZAP_CORE.gamma_multiply(fade)),
            ] {
                for &p in &pixels {
                    let min = board.art_to_screen(p - Vec2::splat((size - 1.0) / 2.0));
                    let rect = Rect::from_min_size(min, Vec2::splat(size * s));
                    painter.rect_filled(rect, CornerRadius::ZERO, color);
                }
            }
        }
    }

    /// Горящие кратеры: огонь по краям клетки (`over` — передние языки, над фигурами). Видны
    /// с касания метеорита до конца шагов хода соперника; кратер позиции без записи (после
    /// отмены) — всегда.
    fn paint_craters(
        &self,
        painter: &Painter,
        board: &BoardGeom,
        tex: &Textures,
        over: bool,
        now: f64,
    ) {
        let mask = self.game.position().craters();
        for sq in 0..64u8 {
            let entry = self.craters.iter().find(|c| c.0 == sq);
            let visible = match entry {
                Some(&(_, start, end)) => now >= start && end.is_none_or(|e| now < e),
                None => mask & 1 << sq != 0,
            };
            if !visible {
                continue;
            }
            let frame = (now / CRATER_FRAME_TIME) as usize + sq as usize * 5;
            let Some(sprite) = tex.ui.crater(over, frame % CRATER_FRAMES) else {
                continue;
            };
            let origin = board.snap(board.ground(sq) - CRATER_CENTER * board.scale);
            let rect =
                Rect::from_min_size(origin + sprite.at * board.scale, sprite.size * board.scale);
            painter.image(sprite.texture, rect, UV, Color32::WHITE);
        }
    }

    /// Волны метеоритов крестом: огонь расходится от клетки падения по четырём соседним.
    fn paint_meteor_waves(
        &self,
        painter: &Painter,
        board: &BoardGeom,
        tex: &Textures,
        over: bool,
        now: f64,
    ) {
        for &(sq, start) in &self.meteor_waves {
            if now < start {
                continue;
            }
            let frame = ((now - start) / WAVE_FRAME_TIME) as usize;
            let Some(sprite) = tex.ui.meteor_wave(over, frame) else {
                continue;
            };
            let origin = board.snap(board.ground(sq) - WAVE_CENTER * board.scale);
            let rect =
                Rect::from_min_size(origin + sprite.at * board.scale, sprite.size * board.scale);
            painter.image(sprite.texture, rect, UV, Color32::WHITE);
        }
    }

    /// Метеориты магов: падают и взрываются на клетке — поверх фигур.
    fn paint_meteors(&self, painter: &Painter, board: &BoardGeom, tex: &Textures, now: f64) {
        for &(square, start, _, _) in &self.meteors {
            if now < start {
                continue;
            }
            let Some(sprite) = tex.ui.meteor(meteor_frame(start, now)) else {
                continue;
            };
            let origin = board.snap(board.ground(square) - METEOR_CENTER * board.scale);
            let rect =
                Rect::from_min_size(origin + sprite.at * board.scale, sprite.size * board.scale);
            painter.image(sprite.texture, rect, UV, Color32::WHITE);
        }
    }

    /// Rogue-like: могилы зомби — бледный череп на клетке, мерцает (через ход зомби восстанет).
    fn paint_graves(&self, painter: &Painter, board: &BoardGeom, tex: &Textures, now: f64) {
        let pos = self.game.position();
        let px = board.scale * 1.5;
        let glow = 0.7 + 0.3 * (now * 2.5).sin() as f32;
        for sq in 0..64u8 {
            if pos.grave(sq).is_none() || self.struck_piece(sq, now).is_some() {
                continue;
            }
            let width = self.art.glyph_width(Glyph::Skull) * px;
            let corner =
                board.snap(board.ground(sq) - vec2(width / 2.0, (GLYPH_HEIGHT + 2.0) * px));
            self.pixel_text(
                painter,
                tex,
                corner,
                px,
                &[(Glyph::Skull, GRAVE_SKULL.gamma_multiply(glow))],
            );
        }
    }

    /// Тёмно-зелёный череп над каждой отравленной фигурой.
    fn paint_skulls(
        &self,
        painter: &Painter,
        board: &BoardGeom,
        tex: &Textures,
        figures: &[Figure],
    ) {
        let px = board.scale * 1.5;
        for f in figures
            .iter()
            .filter(|f| f.piece.poison > 0 && f.alpha > 0.5)
        {
            let height = self.art.height(f.piece.color, f.piece.kind);
            let top = board.to_screen(f.at) - vec2(0.0, height + 4.0) * board.scale;
            let width = self.art.glyph_width(Glyph::Skull) * px;
            let corner = board.snap(top - vec2(width / 2.0, GLYPH_HEIGHT * px));
            self.pixel_text(painter, tex, corner, px, &[(Glyph::Skull, POISON_SKULL)]);
        }
    }

    /// Тени, затем фигуры от дальних к ближним.
    fn paint_figures(
        &self,
        painter: &Painter,
        board: &BoardGeom,
        tex: &Textures,
        figures: &mut [Figure],
    ) {
        figures.sort_by(|a, b| board.depth(a.at).total_cmp(&board.depth(b.at)));
        // Основание — на физическом пикселе: едущая фигура не размывается.
        let ground =
            |f: &Figure| board.snap(board.to_screen(f.at) + vec2(f.shake * board.unit(), 0.0));
        let tint = |f: &Figure| Color32::from_white_alpha((f.alpha.clamp(0.0, 1.0) * 255.0) as u8);
        for f in figures.iter().filter(|f| f.shadow) {
            let texture = tex.piece_shadow(f.piece);
            painter.image(texture, board.sprite_rect(ground(f), f.grow), UV, tint(f));
        }
        for f in figures.iter() {
            if let Some(sink) = f.sink.filter(|s| s.depth > 0.0) {
                self.paint_sunk(painter, board, tex, f.piece, ground(f), sink);
                continue;
            }
            if let Some(frame) = f.bless {
                // Слон с крыльями и ореолом — в своём кадре вокруг точки опоры.
                let min = ground(f) - BLESS_ANCHOR * board.scale;
                let rect = Rect::from_min_size(min, BLESS_FIGURE * board.scale);
                let texture = tex.ui.bless_figure(f.piece.color, frame).id();
                painter.image(texture, rect, UV, tint(f));
                continue;
            }
            // Усиленный компьютер: слон в электричестве — в своём кадре вокруг точки опоры,
            // пешка с лавой — кадр вместо спрайта. Замёрзшие — во льду, как все.
            if f.ice.is_none() {
                if let Some(dark) = f.dark {
                    let (phase, frame) = match dark {
                        Dark::Aura(i) => (0, i),
                        Dark::Out(i) => (1, i),
                        Dark::In(i) => (2, i),
                    };
                    if let Some(sprite) = tex.ui.dark(phase, f.piece, frame) {
                        let origin = ground(f) - ELECTRIC_ANCHOR * board.scale;
                        let rect = Rect::from_min_size(
                            origin + sprite.at * board.scale,
                            sprite.size * board.scale,
                        );
                        painter.image(sprite.texture, rect, UV, tint(f));
                    }
                    // Пустой кадр шага — фигуры нет; пустой кадр ауры — обычная фигура.
                    if !matches!(dark, Dark::Aura(_)) {
                        continue;
                    }
                    if tex.ui.dark(phase, f.piece, frame).is_some() {
                        continue;
                    }
                }
                if let Some(frame) = f.poison
                    && let Some(sprite) = tex.ui.poison(f.piece.color, f.piece.kind, frame)
                {
                    let origin = ground(f) - ELECTRIC_ANCHOR * board.scale;
                    let rect = Rect::from_min_size(
                        origin + sprite.at * board.scale,
                        sprite.size * board.scale,
                    );
                    painter.image(sprite.texture, rect, UV, tint(f));
                    continue;
                }
                if let Some(frame) = f.wind
                    && let Some(sprite) = tex.ui.wind(f.piece, frame)
                {
                    let origin = ground(f) - ELECTRIC_ANCHOR * board.scale;
                    let rect = Rect::from_min_size(
                        origin + sprite.at * board.scale,
                        sprite.size * board.scale,
                    );
                    painter.image(sprite.texture, rect, UV, tint(f));
                    continue;
                }
                if let Some(frame) = f.spark
                    && let Some(sprite) = tex.ui.electric(f.piece, frame)
                {
                    let origin = ground(f) - ELECTRIC_ANCHOR * board.scale;
                    let rect = Rect::from_min_size(
                        origin + sprite.at * board.scale,
                        sprite.size * board.scale,
                    );
                    painter.image(sprite.texture, rect, UV, tint(f));
                    continue;
                }
                if let Some(frame) = f.lava
                    && let Some(sprite) = tex.ui.lava(f.piece, frame)
                {
                    let body = board.sprite_rect(ground(f), f.grow);
                    let px = body.width() / SPRITE.x;
                    let rect = Rect::from_min_size(body.min + sprite.at * px, sprite.size * px);
                    painter.image(sprite.texture, rect, UV, tint(f));
                    continue;
                }
            }
            let rect = board.sprite_rect(ground(f), f.grow);
            match f.ice {
                Some(ice) => self.paint_iced(painter, board, tex, f, ground(f), ice, tint(f)),
                None => {
                    let texture = tex.piece_body(f.piece);
                    painter.image(texture, rect, UV, tint(f));
                }
            }
        }
    }

    /// Фигура во льду, как в breach_freeze.py: иней поднимается от основания (ниже кромки
    /// — фигура в инее, по кромке — белая полоса), поверх растёт глыба. Оттаивая, лёд и иней
    /// тают поверх обычной фигуры.
    #[allow(clippy::too_many_arguments)]
    fn paint_iced(
        &self,
        painter: &Painter,
        board: &BoardGeom,
        tex: &Textures,
        f: &Figure,
        ground: Pos2,
        ice: Ice,
        tint: Color32,
    ) {
        let (color, kind) = (f.piece.color, f.piece.kind);
        let rect = board.sprite_rect(ground, f.grow);
        let frost = FREEZE_FROST[ice.frame.min(FREEZE_FROST.len() - 1)];
        let iced = tint.gamma_multiply(ice.alpha);
        if frost < 1.0 || ice.alpha < 1.0 {
            painter.image(tex.piece_body(f.piece), rect, UV, tint);
        }
        if frost > 0.0 {
            // Кромка — доля высоты фигуры (по белой фигуре, как в генераторе) от основания.
            let row = if frost >= 1.0 {
                0.0
            } else {
                (ANCHOR.y - frost * (self.art.height(Color::White, kind) + 2.0))
                    .ceil()
                    .clamp(0.0, SPRITE.y)
            };
            let band = |from: f32, to: f32| {
                let y = |r: f32| rect.min.y + rect.height() * r / SPRITE.y;
                let screen = Rect::from_min_max(pos2(rect.min.x, y(from)), pos2(rect.max.x, y(to)));
                let uv = Rect::from_min_max(pos2(0.0, from / SPRITE.y), pos2(1.0, to / SPRITE.y));
                (screen, uv)
            };
            let (screen, uv) = band(row, SPRITE.y);
            painter.image(tex.frozen(color, kind), screen, uv, iced);
            if frost < 1.0 {
                let (screen, uv) = band(row, row + 1.0);
                painter.image(tex.frost_edge(color, kind, false), screen, uv, iced);
                let (screen, uv) = band(row + 1.0, SPRITE.y);
                painter.image(tex.frost_edge(color, kind, true), screen, uv, iced);
            }
        }
        // Глыба льда — поверх фигуры, вокруг её клетки.
        if let Some(block) = tex.ui.freeze_block(kind, ice.variant, ice.frame) {
            let min = ground + (block.at - FREEZE_CENTER) * board.scale;
            let rect = Rect::from_min_size(min, block.size * board.scale);
            painter.image(block.texture, rect, UV, iced);
        }
    }

    /// Лёд на фигуре `piece` на клетке `sq`: она заморожена или лёд на ней ещё тает.
    fn ice(&self, sq: Square, piece: Piece, now: f64) -> Option<Ice> {
        self.ice_on(sq, piece, now).map(|(frame, alpha)| Ice {
            frame,
            variant: ice_variant(sq),
            alpha,
        })
    }

    /// Фигура, ушедшая в портал на `sink.depth`, как `sunk_piece` в breach_portal.py:
    /// видна только часть выше передней кромки кольца, а пиксели фигуры на самой кромке
    /// светятся. Кромка — полуэллипс, поэтому у каждого столбца пикселей своя высота среза.
    fn paint_sunk(
        &self,
        painter: &Painter,
        board: &BoardGeom,
        tex: &Textures,
        piece: Piece,
        ground: Pos2,
        sink: Sink,
    ) {
        let s = board.scale;
        let texture = tex.piece_body(piece);
        let top_left = ground - ANCHOR * s + vec2(0.0, sink.depth * s);
        let (rx, ry) = (PORTAL_RADII.x * sink.scale, PORTAL_RADII.y * sink.scale);
        // Спрайт в кадре портала: его левый верхний угол без погружения.
        let shift = PORTAL_CENTER - ANCHOR;
        let cut_color = Color32::from_rgb(PORTAL_CUT[0], PORTAL_CUT[1], PORTAL_CUT[2]);
        for x in 0..SPRITE.x as usize {
            let dx = x as f32 + shift.x + 0.5 - PORTAL_CENTER.x;
            if rx <= 0.0 || dx.abs() >= rx {
                continue; // вне кольца фигура срезана целиком
            }
            let rim = PORTAL_CENTER.y + ry * (1.0 - (dx / rx).powi(2)).sqrt();
            // Видны строки кадра y + 0.5 < rim — в строках спрайта столько сверху.
            let rows = ((rim - 0.5).ceil() - shift.y - sink.depth).clamp(0.0, SPRITE.y);
            if rows <= 0.0 {
                continue;
            }
            let column = Rect::from_min_size(top_left + vec2(x as f32 * s, 0.0), vec2(s, rows * s));
            let uv = Rect::from_min_max(
                pos2(x as f32 / SPRITE.x, 0.0),
                pos2((x + 1) as f32 / SPRITE.x, rows / SPRITE.y),
            );
            painter.image(texture, column, uv, Color32::WHITE);
            let last = rows - 1.0;
            if self
                .art
                .covers(piece.color, piece.kind, vec2(x as f32, last))
            {
                let pixel =
                    Rect::from_min_size(top_left + vec2(x as f32, last) * s, Vec2::splat(s));
                painter.rect_filled(pixel, CornerRadius::ZERO, cut_color);
            }
        }
    }

    /// Точка над макушкой фигуры на клетке `sq` — сюда ставятся числа.
    fn label_point(&self, board: &BoardGeom, sq: Square, piece: Option<Piece>) -> Pos2 {
        let height = piece.map_or(36.0, |p| self.art.height(p.color, p.kind));
        board.ground(sq) - vec2(0.0, height + 3.0) * board.scale
    }

    /// Ширина строки пиксельного шрифта в пикселях шрифта: знаки через пиксель.
    fn text_width(&self, text: &[(Glyph, Color32)]) -> f32 {
        let glyphs: f32 = text.iter().map(|&(g, _)| self.art.glyph_width(g)).sum();
        glyphs + text.len().saturating_sub(1) as f32
    }

    /// Строка пиксельного шрифта: `corner` — левый верхний угол знаков без контура,
    /// `px` — точек экрана на пиксель шрифта, у каждого знака свой цвет.
    fn pixel_text(
        &self,
        painter: &Painter,
        tex: &Textures,
        corner: Pos2,
        px: f32,
        text: &[(Glyph, Color32)],
    ) {
        let mut x = 0.0;
        for &(glyph, color) in text {
            let width = self.art.glyph_width(glyph);
            let rect = Rect::from_min_size(
                corner + vec2(x - 1.0, -1.0) * px,
                vec2(width + 2.0, GLYPH_HEIGHT + 3.0) * px,
            );
            painter.image(tex.glyph(glyph), rect, UV, color);
            x += width + 1.0;
        }
    }

    /// Атака слева от фигуры, здоровье справа — в пиксельных табличках, как в Into the
    /// Breach. Таблички выезжают из-за фигуры с лёгким перелётом.
    fn paint_stats(
        &self,
        painter: &Painter,
        board: &BoardGeom,
        tex: &Textures,
        sq: Square,
        piece: Piece,
        age: f64,
    ) {
        // В маленьком окне таблички не мельче, чем нужно для чтения.
        let s = board.scale.max(1.75);
        let t = (age / STATS_TIME).clamp(0.0, 1.0) as f32;
        let out = ease_out_back(t);
        let alpha = (t * 2.0).min(1.0);
        let height = self.art.height(piece.color, piece.kind);
        // Середина фигуры, но не выше макушки пешки: таблички не уходят к соседям сверху.
        let middle = board.ground(sq) - vec2(0.0, (height * 0.5).min(24.0)) * s;
        let hp_color = hp_color(piece.hp as f32 / piece.max_hp() as f32);
        let attack = [
            &[(Glyph::Sword, PIXEL_STEEL)][..],
            // Урон удара (с 0.119 у фигур игрока — атака × мульти).
            &paint(
                &Glyph::digits(shown(self.game.position().strike_damage(piece))),
                PIXEL_WHITE,
            ),
        ]
        .concat();
        let health = [
            &[(Glyph::Heart, hp_color)][..],
            &paint(&Glyph::digits(shown(piece.hp)), hp_color),
        ]
        .concat();
        for (text, left) in [(attack, true), (health, false)] {
            let size = self.plate_size(&text);
            let gap = 13.0 + 8.0 * (1.0 - out);
            let x = if left { -gap - size.x } else { gap };
            let rise = 4.0 * (1.0 - out);
            let min = middle + vec2(x, -size.y / 2.0 + rise) * s;
            self.paint_plate(painter, tex, min, s, &text, alpha);
        }
        // Герб красной молнии: заряд электрической фигуры — над головой, молния и число.
        if piece.spark && self.game.position().has_crest(Crest::RedBolt) {
            let charge = [
                &[(Glyph::Bolt, PIXEL_BOLT)][..],
                &paint(&Glyph::digits(i32::from(piece.charge)), PIXEL_WHITE),
            ]
            .concat();
            let size = self.plate_size(&charge);
            let head = board.ground(sq) - vec2(0.0, height + 4.0) * board.scale;
            let rise = 6.0 * (1.0 - out);
            let min = head - vec2(size.x / 2.0, size.y - rise) * s;
            self.paint_plate(painter, tex, min, s, &charge, alpha);
        }
    }

    /// Размер таблички с пиксельной строкой `text`, в пикселях шрифта.
    fn plate_size(&self, text: &[(Glyph, Color32)]) -> Vec2 {
        vec2(self.text_width(text) + 7.0, GLYPH_HEIGHT + 7.0)
    }

    /// Табличка как в Into the Breach: чёрная рамка, светлая кайма, тёмное поле, знаки с
    /// отступом 3. `min` — левый верхний угол, `s` — точек экрана на пиксель шрифта.
    fn paint_plate(
        &self,
        painter: &Painter,
        tex: &Textures,
        min: Pos2,
        s: f32,
        text: &[(Glyph, Color32)],
        alpha: f32,
    ) {
        let size = self.plate_size(text);
        let plate = |dx: f32, dy: f32, w: f32, h: f32, color: u32| {
            let rect = Rect::from_min_size(min + vec2(dx, dy) * s, vec2(w, h) * s);
            painter.rect_filled(rect, CornerRadius::ZERO, hex(color).gamma_multiply(alpha));
        };
        plate(0.0, 0.0, size.x, size.y, 0x08090d);
        plate(1.0, 1.0, size.x - 2.0, size.y - 2.0, 0xc9ced8);
        plate(2.0, 2.0, size.x - 4.0, size.y - 4.0, 0x161a24);
        let text: Vec<(Glyph, Color32)> = text
            .iter()
            .map(|&(g, c)| (g, c.gamma_multiply(alpha)))
            .collect();
        self.pixel_text(painter, tex, min + vec2(3.0, 3.0) * s, s, &text);
    }

    /// Цифра урона или лечения, всплывающая над фигурой: выскакивает крупной,
    /// подпрыгивает, оседает и гаснет.
    fn paint_damage_number(
        &self,
        painter: &Painter,
        board: &BoardGeom,
        tex: &Textures,
        anchor: Pos2,
        effect: &Effect,
        now: f64,
    ) {
        let age = now - effect.start;
        if !(0.0..EFFECT_TIME).contains(&age) {
            return;
        }
        let u = (age / EFFECT_TIME) as f32;
        let jump = ease_out_back((u / 0.3).min(1.0));
        // Цифра над ещё видными цифрами той же клетки — на строку выше.
        let stack = f32::from(effect.stack) * (GLYPH_HEIGHT + 2.0) * 1.5;
        let rise = (jump * 14.0 + u * 6.0 + stack) * board.scale;
        let pop = 1.0 + 0.5 * (1.0 - (u / 0.15).min(1.0));
        let alpha = if u < 0.65 {
            1.0
        } else {
            1.0 - (u - 0.65) / 0.35
        };
        let amount = Glyph::digits(shown(effect.amount));
        let (lead, color): (&[Glyph], Color32) = match effect.mark {
            _ if effect.killed => (&[Glyph::Skull, Glyph::Minus], PIXEL_RED),
            Mark::Hit => (&[Glyph::Minus], PIXEL_GOLD),

            Mark::Heal => (&[Glyph::Plus], PIXEL_GREEN),
        };
        let text: Vec<(Glyph, Color32)> = [lead, &amount]
            .concat()
            .into_iter()
            .map(|g| (g, color.gamma_multiply(alpha)))
            .collect();
        let px = board.scale * 1.5 * pop;
        let width = self.text_width(&text) * px;
        let corner = anchor - vec2(width / 2.0, GLYPH_HEIGHT * px + rise);
        self.pixel_text(painter, tex, corner, px, &text);
    }

    fn paint_dragged(&self, painter: &Painter, board: &BoardGeom, tex: &Textures) {
        let Some(drag) = self.drag.as_ref().filter(|d| d.active) else {
            return;
        };
        if let Some(piece) = self.game.position().piece(drag.from) {
            let rect = board.sprite_rect(drag.pointer + drag.grab, 1.0);
            painter.image(tex.piece_shadow(piece), rect, UV, Color32::WHITE);
            painter.image(tex.piece_body(piece), rect, UV, Color32::WHITE);
        }
    }

    /// Какой курсор показать: над столом он подсказывает, что будет по щелчку.
    fn pointer_kind(&self, board: &BoardGeom, pointer: Option<Pos2>) -> Pointer {
        if self.orbit.is_some() {
            return Pointer::Default;
        }
        if self.holding_card() {
            return Pointer::Card;
        }
        if let Some(drag) = self.drag.as_ref().filter(|d| d.active) {
            return self
                .hovered
                .and_then(|to| self.move_pointer(drag.from, to))
                .unwrap_or(Pointer::Off);
        }
        let Some(pointer) = pointer else {
            return Pointer::Default;
        };
        if let Some(pending) = &self.promotion {
            let choice = self
                .promotion_choices(pending)
                .iter()
                .any(|&(sq, _)| Some(sq) == self.hovered);
            return if choice {
                Pointer::Hover
            } else {
                Pointer::Default
            };
        }
        if self.aiming_card() {
            let target = self.hovered.is_some_and(|sq| self.aim_pickable(sq));
            return if target {
                Pointer::Hover
            } else {
                Pointer::Card
            };
        }
        if let Some(spot) = self.shop_hover {
            return if self.shop_usable(spot) {
                Pointer::Hover
            } else {
                Pointer::Off
            };
        }
        if let Some(item) = self.card_menu_hover {
            return if self.card_menu_usable(item) {
                Pointer::Hover
            } else {
                Pointer::Off
            };
        }
        if self.unfold_hover {
            return Pointer::Hover;
        }
        if self.crest_sell_hover {
            return Pointer::Hover;
        }
        if self.sell_drag.is_some() {
            return Pointer::Move;
        }
        if let Some(slot) = self.power_hover {
            return if self.hand_usable(slot) {
                Pointer::Hover
            } else {
                Pointer::Off
            };
        }
        if let Some((seat, stock)) = self.reserve_hover {
            return if self.reserve_usable(seat, stock) || self.reserve_sellable(seat, stock) {
                Pointer::Hover
            } else {
                Pointer::Off
            };
        }

        if self.button_hover {
            return if self.button_active() {
                Pointer::Hover
            } else {
                Pointer::Off
            };
        }
        if self.card_hover.is_some() {
            return if self.card_playable() {
                Pointer::Card
            } else {
                Pointer::Off
            };
        }
        if self.over_closed_cards(board, pointer) {
            return Pointer::Off;
        }
        let Some(sq) = self.hovered.filter(|_| self.human_to_move()) else {
            return Pointer::Default;
        };
        if let Some(kind) = self.selected.and_then(|from| self.move_pointer(from, sq)) {
            return kind;
        }
        let pos = self.game.position();
        let own = pos.piece(sq).filter(|p| p.color == pos.side());
        if pos.can_move(sq) && own.is_some_and(|p| !p.is_frozen()) {
            Pointer::Hover
        } else if own.is_some_and(|p| p.is_frozen()) {
            // Замёрзшая своя фигура не ходит.
            Pointer::Off
        } else if self.selected.is_some() {
            // Выбранная фигура сюда не ходит.
            Pointer::Off
        } else {
            Pointer::Default
        }
    }

    /// Курсор хода, если фигура с `from` может пойти на `to`.
    fn move_pointer(&self, from: Square, to: Square) -> Option<Pointer> {
        self.has_move(from, to).then_some(Pointer::Move)
    }

    /// Пиксельная стрелка из арт-проекта вместо системного курсора — поверх всего окна.
    /// Особые курсоры egui (текст, растягивание поля) остаются системными.
    fn paint_cursor(&self, ctx: &egui::Context) {
        let Some(pos) = ctx.input(|i| i.pointer.hover_pos()) else {
            return;
        };
        if ctx.output(|o| o.cursor_icon) != CursorIcon::Default {
            return;
        }
        ctx.set_cursor_icon(CursorIcon::None);
        let ppp = ctx.pixels_per_point();
        // Ровно `cursor_scale` физических пикселей на пиксель арта — как у доски.
        let scale = self.cursor_scale as f32 / ppp;
        let tip = pos2((pos.x * ppp).round() / ppp, (pos.y * ppp).round() / ppp);
        let rect = Rect::from_min_size(tip - CURSOR_HOTSPOT * scale, CURSOR * scale);
        let texture = self.art.cursor(ctx, self.pointer, self.cursor_scale);
        let layer = egui::LayerId::new(egui::Order::Debug, egui::Id::new("cursor"));
        ctx.layer_painter(layer)
            .image(texture, rect, UV, Color32::WHITE);
    }

    /// Дрожь фигуры после удара или ответа (в долях клетки).
    fn shake(&self, sq: Square, now: f64) -> f32 {
        self.effects
            .iter()
            .filter(|e| e.wounds() && e.square == sq)
            .map(|e| {
                let age = now - e.start;
                if !(0.0..FLASH_TIME).contains(&age) {
                    0.0
                } else {
                    ((age * 70.0).sin() * 0.06 * (1.0 - age / FLASH_TIME)) as f32
                }
            })
            .sum()
    }

    /// Выбор фигуры для превращения: фигуры на выбор стоят на клетках вертикали.
    fn paint_promotion(
        &self,
        painter: &Painter,
        board: &BoardGeom,
        tex: &Textures,
        pending: &PendingPromotion,
    ) {
        painter.rect_filled(painter.clip_rect(), CornerRadius::ZERO, rgba(0, 0, 0, 0.55));
        let color = self.game.position().side();
        let mut figures = Vec::new();
        for (sq, kind) in self.promotion_choices(pending) {
            let hovered = self.hovered == Some(sq);
            let fill = if hovered {
                rgba(214, 79, 0, 0.8)
            } else {
                rgba(176, 176, 176, 0.45)
            };
            painter.image(tex.marker(Marker::Fill), board.tile_rect(sq), UV, fill);
            if hovered {
                painter.image(
                    tex.marker(Marker::Target),
                    board.tile_rect(sq),
                    UV,
                    Color32::WHITE,
                );
            }
            figures.push(Figure {
                grow: if hovered { 1.0 } else { 0.85 },
                ..Figure::new(center(sq), Piece::new(kind, color))
            });
        }
        self.paint_figures(painter, board, tex, &mut figures);
    }

    fn paint_banner(&self, painter: &Painter, board: &BoardGeom, now: f64) {
        let Some(ended) = self.ended_at else {
            return;
        };
        let age = now - ended;
        if age > BANNER_TIME {
            return;
        }
        let alpha = (age / 0.25).min((BANNER_TIME - age) / 0.5).min(1.0) as f32;
        let (title, subtitle) = result_text(self.game.status(), self.game.position());
        let unit = board.unit();
        let rect = Rect::from_center_size(board.rect().center(), vec2(unit * 5.4, unit * 1.7));
        painter.rect_filled(rect, CornerRadius::same(8), rgba(22, 21, 18, 0.9 * alpha));
        painter.text(
            rect.center() - vec2(0.0, rect.height() * 0.16),
            Align2::CENTER_CENTER,
            title,
            FontId::new(unit * 0.46, theme::bold()),
            theme::TEXT_BRIGHT.gamma_multiply(alpha),
        );
        painter.text(
            rect.center() + vec2(0.0, rect.height() * 0.24),
            Align2::CENTER_CENTER,
            subtitle,
            FontId::proportional(unit * 0.22),
            theme::TEXT.gamma_multiply(alpha),
        );
    }

    // ----- Правая панель: игроки, ходы, кнопки, сведения о фигуре -----

    fn game_panel(&mut self, ui: &mut egui::Ui) {
        let top = if self.flipped {
            Color::White
        } else {
            Color::Black
        };
        egui::TopBottomPanel::top("top-player")
            .frame(egui::Frame::default())
            .show_separator_line(false)
            .show_inside(ui, |ui| {
                self.player_card(ui, top);
                ui.add_space(8.0);
            });
        egui::TopBottomPanel::bottom("bottom-player")
            .frame(egui::Frame::default())
            .show_separator_line(false)
            .resizable(false)
            .show_inside(ui, |ui| {
                ui.add_space(8.0);
                self.player_card(ui, top.opponent());
                ui.add_space(8.0);
                self.controls(ui);
                if self.game.position().rogue().is_some() && self.remote.is_none() {
                    ui.add_space(6.0);
                    self.rogue_controls(ui);
                }
                ui.add_space(8.0);
                self.info_card(ui);
            });
        // Список ходов занимает всё, что осталось посередине.
        egui::CentralPanel::default()
            .frame(egui::Frame::default())
            .show_inside(ui, |ui| self.moves_card(ui));
    }

    fn role(&self, color: Color) -> String {
        if let Some(remote) = self.remote {
            return if remote == color {
                "соперник по сети".to_owned()
            } else {
                "вы".to_owned()
            };
        }
        if let Some(r) = self.game.position().rogue()
            && r.player != color
        {
            if r.shop {
                return format!("магазин перед уровнем {}", r.level + 1);
            }
            if r.planning {
                return format!("расстановка перед уровнем {}", r.level + 1);
            }
            return format!(
                "уровень {} из {} · волна {} из {}",
                r.level, r.levels, r.wave, r.waves
            );
        }
        match self.computer {
            Some(computer) if computer == color => "компьютер".to_owned(),
            Some(_) => "вы".to_owned(),
            None => "игрок".to_owned(),
        }
    }

    /// Карточка игрока: имя и чей ход, кто играет и сколько здоровья у армии, убитые враги.
    fn player_card(&self, ui: &mut egui::Ui, color: Color) {
        let pos = self.game.position();
        let active = !self.game.is_over() && pos.side() == color;
        card(ui, |ui| {
            ui.horizontal(|ui| {
                let (dot, _) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
                let fill = if color == Color::White {
                    Color32::from_gray(240)
                } else {
                    Color32::from_gray(16)
                };
                let outline = Stroke {
                    width: 1.0,
                    color: Color32::from_gray(120),
                };
                ui.painter().circle(dot.center(), 6.0, fill, outline);
                let name_color = if active {
                    theme::TEXT_BRIGHT
                } else {
                    theme::TEXT
                };
                ui.label(
                    RichText::new(color.name())
                        .size(16.0)
                        .family(theme::bold())
                        .color(name_color),
                );
                if active {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(RichText::new("ходит").color(theme::GREEN));
                    });
                }
            });
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(self.role(color))
                        .size(12.5)
                        .color(theme::TEXT_DIM),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    // Сумма без переполнения: у фигур волн дальних уровней здоровья очень много.
                    let health = pos
                        .pieces()
                        .filter(|(_, p)| p.color == color)
                        .fold(0i32, |sum, (_, p)| sum.saturating_add(p.hp));
                    ui.label(
                        RichText::new(format!("армия: {} HP", shown(health)))
                            .size(12.5)
                            .color(theme::TEXT_DIM),
                    );
                });
            });
            let mut kills = self.game.kills_by(color);
            if !kills.is_empty() {
                kills.sort_by_key(|p| std::cmp::Reverse(p.kind as u8));
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    for piece in &kills {
                        piece_icon(ui, &self.art, piece.color, piece.kind, 20.0);
                    }
                });
            }
        });
    }

    fn moves_card(&self, ui: &mut egui::Ui) {
        let height = ui.available_height();
        egui::Frame::default()
            .fill(theme::PANEL)
            .corner_radius(CornerRadius::same(4))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.set_height(height);
                egui::Frame::default()
                    .inner_margin(Margin::symmetric(10, 8))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        self.status_line(ui);
                    });
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .stick_to_bottom(true)
                    .show(ui, |ui| self.move_rows(ui));
            });
    }

    fn status_line(&self, ui: &mut egui::Ui) {
        let pos = self.game.position();
        let side = pos.side();
        let (text, color) = match self.game.status() {
            Status::Ongoing if self.aiming_card() => {
                (self.aim_hint().to_owned(), theme::TEXT_BRIGHT)
            }
            Status::Ongoing if self.picked_card().is_some() => (
                match (self.picked_card(), self.mirror_from) {
                    (Some(game::Card::Mirror), Some(from)) => format!(
                        "Карта зеркала: щёлкните по фигуре, копией которой станет фигура на {}",
                        game::square_name(from)
                    ),
                    (Some(game::Card::Mirror), None) => {
                        "Карта зеркала: щёлкните по своей фигуре, которая станет копией другой"
                            .to_owned()
                    }
                    (card, _) => format!(
                        "{}: щёлкните по своей подсвеченной фигуре (повторный щелчок по карте — \
                         отмена)",
                        card.map_or("", |c| c.title())
                    ),
                },
                theme::GOLD,
            ),
            Status::Ongoing if pos.chest_open().is_some() => (
                match pos.chest_open().and_then(|slot| pos.chest(slot)) {
                    Some(offer) => format!(
                        "{}: выберите {} из {} (осталось {}) — щёлкните по вещи",
                        offer.title(),
                        offer.picks(),
                        offer.choices(),
                        offer.picks() - usize::from(offer.taken)
                    ),
                    None => String::new(),
                },
                theme::GOLD,
            ),
            Status::Ongoing if pos.shop_open() => (
                "Магазин: сундуки фигур, гербов, карт и медалей — выберите одно из трёх (из \
                 большого — два из пяти); «Обновить» — новые сундуки; продавайте фигуры запаса \
                 (перетащите в «Продажу»), гербы (щёлкните по гербу слева) и карты (щёлкните по \
                 карте — «Продать»; там же «Сыграть»); «свернуть» открывает доску; «Уйти» — к \
                 расстановке"
                    .to_owned(),
                theme::GOLD,
            ),
            Status::Ongoing if pos.planning() => (
                format!(
                    "Расстановка перед уровнем {} · двигайте и меняйте фигуры местами на двух \
                     первых горизонталях, потом \
                     «Начать раунд»",
                    pos.rogue().map_or(0, |r| r.level + 1)
                ),
                theme::TEXT_BRIGHT,
            ),
            Status::Ongoing
                if pos.moved() && self.human_to_move() && !self.game.legal_moves().is_empty() =>
            {
                (
                    format!(
                        "Ход {} · ещё ходов: {} (другими фигурами) — или закончите ход",
                        side.genitive(),
                        pos.moves_left()
                    ),
                    theme::TEXT_BRIGHT,
                )
            }
            Status::Ongoing if pos.moved() && self.human_to_move() => (
                format!("Ход {} · закончите ход", side.genitive()),
                theme::TEXT_BRIGHT,
            ),
            Status::Ongoing if self.game.legal_moves().is_empty() && self.human_to_move() => (
                format!(
                    "Ход {} · ходить нечем — сыграйте карту или закончите ход",
                    side.genitive()
                ),
                theme::ORANGE,
            ),
            Status::Ongoing if pos.king_square(side).is_none() => (
                format!("Ход {} · короля нет — карты не сыграть", side.genitive()),
                theme::ORANGE,
            ),
            Status::Ongoing if pos.in_check(side) => (
                format!(
                    "Шах! Короля {} могут убить следующим ходом",
                    side.genitive()
                ),
                theme::RED,
            ),
            Status::Ongoing => match pos.king_danger(side) {
                0 => (format!("Ход {}", side.genitive()), theme::TEXT_BRIGHT),
                danger => (
                    format!(
                        "Ход {} · король под ударом (до −{} HP)",
                        side.genitive(),
                        shown(danger)
                    ),
                    theme::ORANGE,
                ),
            },
            status => {
                let (title, subtitle) = result_text(status, self.game.position());
                (format!("{title}. {subtitle}"), theme::TEXT_BRIGHT)
            }
        };
        ui.label(RichText::new(text).size(15.0).color(color));
        if pos.advance() {
            ui.label(
                RichText::new("Взятие с перемещением: пешка, конь и король встают на место убитой")
                    .size(12.0)
                    .color(theme::TEXT_DIM),
            );
        }
        if let Some(note) = &self.net_note {
            ui.label(RichText::new(note).size(13.0).color(theme::ORANGE));
        }
    }

    /// Список ходов в две колонки, как на lichess: в ячейке — ход фигурой и карта
    /// стороны. Подробности — во всплывающей подсказке.
    fn move_rows(&self, ui: &mut egui::Ui) {
        let turns = group_turns(self.game.history());
        let width = ui.available_width();
        let number_width = 34.0;
        let cell_width = (width - number_width) / 2.0;
        for (row, pair) in turns.chunks(2).enumerate() {
            let galleys: Vec<_> = pair
                .iter()
                .enumerate()
                .map(|(i, turn)| {
                    let last = row * 2 + i + 1 == turns.len();
                    let job = turn_job(turn, last, cell_width - 12.0);
                    ui.ctx().fonts(|f| f.layout_job(job))
                })
                .collect();
            let row_height = galleys
                .iter()
                .map(|g| g.size().y + 8.0)
                .fold(26.0, f32::max);
            let (rect, _) = ui.allocate_exact_size(vec2(width, row_height), Sense::hover());
            let painter = ui.painter().clone();
            let number = Rect::from_min_size(rect.min, vec2(number_width, row_height));
            painter.rect_filled(number, CornerRadius::ZERO, theme::PANEL_ALT);
            painter.text(
                number.center(),
                Align2::CENTER_CENTER,
                (row + 1).to_string(),
                FontId::proportional(13.0),
                theme::TEXT_DIM,
            );
            for (i, (turn, galley)) in pair.iter().zip(galleys).enumerate() {
                let index = row * 2 + i;
                let cell = Rect::from_min_size(
                    rect.min + vec2(number_width + cell_width * i as f32, 0.0),
                    vec2(cell_width, row_height),
                );
                let response = ui.interact(cell, ui.id().with(("move", index)), Sense::hover());
                if index + 1 == turns.len() {
                    painter.rect_filled(cell, CornerRadius::ZERO, rgba(41, 58, 73, 1.0));
                } else if response.hovered() {
                    painter.rect_filled(cell, CornerRadius::ZERO, theme::PANEL_ALT);
                }
                painter.galley(
                    cell.left_center() + vec2(8.0, -galley.size().y / 2.0),
                    galley,
                    theme::TEXT,
                );
                let text: Vec<String> = turn.iter().map(|r| r.description()).collect();
                let _ = response.on_hover_text(text.join("\n"));
            }
        }
        if turns.is_empty() {
            ui.add_space(10.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("Ходов пока нет").color(theme::TEXT_DIM))
            });
        }
        if self.game.is_over() {
            ui.add_space(8.0);
            let status = self.game.status();
            let (title, subtitle) = result_text(status, self.game.position());
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new(score_text(status))
                        .size(18.0)
                        .family(theme::bold())
                        .color(theme::TEXT_BRIGHT),
                );
                ui.label(
                    RichText::new(format!("{title} · {subtitle}"))
                        .italics()
                        .color(theme::TEXT_DIM),
                );
            });
            ui.add_space(8.0);
        }
    }

    fn controls(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let can_undo = !self.game.history().is_empty() && self.remote.is_none();
            if ui
                .add_enabled(can_undo, egui::Button::new("↶ Ход назад"))
                .on_hover_text("Отменить ход (Ctrl+Z)")
                .clicked()
            {
                self.undo();
            }
            if ui
                .button("⇅")
                .on_hover_text("Перевернуть доску (F)")
                .clicked()
            {
                self.flip_board();
            }
            let muted = !self.settings.sound || self.settings.volume == 0;
            let icon = if muted { "🔇" } else { "🔊" };
            let volume = ui.button(icon).on_hover_text("Громкость");
            let popup = ui.make_persistent_id("volume-popup");
            if volume.clicked() {
                ui.memory_mut(|m| m.toggle_popup(popup));
            }
            egui::containers::popup::popup_below_widget(
                ui,
                popup,
                &volume,
                egui::PopupCloseBehavior::CloseOnClickOutside,
                |ui| {
                    ui.set_width(220.0);
                    self.volume_controls(ui);
                },
            );
            if ui
                .button("Меню")
                .on_hover_text("Выйти в главное меню (партия начнётся заново)")
                .clicked()
            {
                self.computer_since = None;
                self.leave_network();
                self.orbit = None;
                self.screen = Screen::Menu;
            }
        });
        let look = if self.orbit.is_some() {
            "Закрыть осмотр"
        } else {
            "Осмотр доски"
        };
        if ui
            .button(look)
            .on_hover_text("Крутить доску мышью (V). Колёсико приближает, Esc — обычный вид")
            .clicked()
        {
            self.toggle_orbit();
        }
        if self.orbit.is_some() {
            ui.label(
                RichText::new(
                    "Тяните мышь, чтобы осмотреть доску. Колёсико приближает. Esc — обычный вид. Ходы в осмотре не играются.",
                )
                .color(theme::TEXT_DIM)
                .size(12.0),
            );
        }
    }

    /// Rogue-like: кнопка «Выиграть раунд» — сразу победить волны этого уровня.
    fn rogue_controls(&mut self, ui: &mut egui::Ui) {
        let can = self.human_to_move()
            && self.game.position().can_win_round()
            && self.anim.is_none()
            && self.strikes.is_empty();
        if ui
            .add_enabled(can, egui::Button::new("Выиграть раунд"))
            .on_hover_text("Сразу победить волны этого уровня — золото за фигуры на доске")
            .clicked()
            && self.game.win_round()
        {
            self.reset_interaction();
            self.sfx.play(Sound::Notify);
        }
    }

    /// Атака и здоровье фигуры под курсором (или выбранной).
    fn info_card(&self, ui: &mut egui::Ui) {
        let pos = self.game.position();
        let focus = self
            .hovered
            .filter(|&sq| pos.piece(sq).is_some())
            .or(self.selected)
            .and_then(|sq| Some((sq, pos.piece(sq)?)));
        card(ui, |ui| {
            // С запасом, чтобы панель не прыгала при наведении.
            ui.set_min_height(96.0);
            let Some((sq, piece)) = focus else {
                ui.label(
                    RichText::new(
                        "Наведите курсор на фигуру — здесь появятся её атака и здоровье. \
                         Бьют фигуры сами — в конце хода своей стороны.",
                    )
                    .color(theme::TEXT_DIM),
                );
                return;
            };
            ui.horizontal(|ui| {
                piece_icon(ui, &self.art, piece.color, piece.kind, 46.0);
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new(format!("{} · {}", piece.title(), game::square_name(sq)))
                            .size(15.5)
                            .color(theme::TEXT_BRIGHT),
                    );
                    // Фигуры игрока бьют атакой × мульти (с 0.119).
                    let damage = self.game.position().strike_damage(piece);
                    let attack = if damage == piece.attack() {
                        format!("Атака {}", shown(damage))
                    } else {
                        format!(
                            "Атака {} × {} = {}",
                            shown(piece.attack()),
                            game::mult_text(self.game.position().piece_mult(piece)),
                            shown(damage)
                        )
                    };
                    ui.label(format!(
                        "{attack}  ·  HP {}/{}",
                        shown(piece.hp),
                        shown(piece.max_hp())
                    ));
                    hp_meter(ui, piece.hp, piece.max_hp());
                    // Маг копит метеорит (с 0.108): когда следующий.
                    if piece.wizard {
                        ui.label(match piece.rest + u8::from(piece.fresh) {
                            0 => "Метеорит — в конце хода".to_owned(),
                            1 => "Метеорит — через ход".to_owned(),
                            n @ 2..=4 => format!("Метеорит — через {n} хода"),
                            n => format!("Метеорит — через {n} ходов"),
                        });
                    }
                    // Герб красной молнии: заряд электрической фигуры и как далеко летит молния.
                    let pos = self.game.position();
                    if piece.spark && pos.has_crest(Crest::RedBolt) {
                        ui.label(format!(
                            "Заряд {}  ·  молния летит на {}",
                            piece.charge,
                            match pos.spark_reach(piece) {
                                1 => "1 клетку".to_owned(),
                                n @ 2..=4 => format!("{n} клетки"),
                                n => format!("{n} клеток"),
                            }
                        ));
                    }
                });
            });
        });
    }
    // ----- Разделы экранов «Настройки» и «Правила» главного меню (с 0.114) -----

    pub(super) fn settings_section(&mut self, ui: &mut egui::Ui) {
        option_label(ui, "Громкость");
        self.volume_controls(ui);
        option_label(ui, "Частота кадров");
        ui.horizontal_wrapped(|ui| {
            for rate in FrameRate::ALL {
                let label = rate.label();
                let hint = match rate {
                    FrameRate::Vsync => {
                        "Кадры идут вместе с обновлением монитора \
                                         (144 кадра на мониторе 144 Гц) — плавнее всего"
                    }
                    FrameRate::Cap(_) => {
                        "Без синхронизации с монитором, не чаще этого числа \
                                          кадров — меньше задержка мыши"
                    }
                    FrameRate::Unlimited => {
                        "Без синхронизации и ограничения: кадров столько, \
                                             сколько успевает видеокарта"
                    }
                };
                let _ = ui
                    .selectable_value(&mut self.settings.frame_rate, rate, label)
                    .on_hover_text(hint);
            }
        });
        if self.settings.frame_rate.vsync() != self.vsync {
            ui.label(
                RichText::new(if self.vsync {
                    "VSync выключится после перезапуска игры"
                } else {
                    "VSync включится после перезапуска игры"
                })
                .size(12.5)
                .color(theme::ORANGE),
            );
        }
        ui.checkbox(&mut self.settings.show_fps, "Показывать FPS (F3)");
        option_label(ui, "Масштаб доски (Ctrl/⌘ + колесо над доской)");
        ui.horizontal(|ui| {
            ui.add(
                egui::Slider::new(&mut self.settings.zoom, ZOOM_MIN..=ZOOM_MAX)
                    .suffix("%")
                    .step_by(10.0),
            );
            if ui.button("по окну").clicked() {
                self.settings.zoom = 100;
            }
        });
    }

    pub(super) fn stats_section(&self, ui: &mut egui::Ui) {
        section_title(ui, "Характеристики");
        egui::Grid::new("stats")
            .num_columns(4)
            .min_col_width(0.0)
            .spacing([12.0, 4.0])
            .striped(true)
            .show(ui, |ui| {
                ui.label("");
                for title in ["Фигура", "Атака", "HP"] {
                    ui.label(RichText::new(title).size(12.5).color(theme::TEXT_DIM));
                }
                ui.end_row();
                for kind in Kind::ALL {
                    piece_icon(ui, &self.art, Color::White, kind, 26.0);
                    ui.label(kind.title());
                    let stats = kind.stats();
                    let max_hp = rogue_max_hp(kind);
                    ui.label(
                        RichText::new(shown(stats.attack).to_string()).color(theme::TEXT_BRIGHT),
                    );
                    ui.label(RichText::new(shown(max_hp).to_string()).color(theme::TEXT_BRIGHT));
                    ui.end_row();
                }
            });
        ui.label(
            RichText::new(
                "Удар снимает столько HP, сколько у атакующего атаки. Фигуры бьют сами в конце \
                 хода своей стороны, ответа нет.",
            )
            .size(12.5)
            .color(theme::TEXT_DIM),
        );
    }

    pub(super) fn damage_section(&self, ui: &mut egui::Ui) {
        section_title(ui, "Ударов до убийства");
        ui.label(
            RichText::new(
                "Строка — кто бьёт, столбец — по кому: сколько ударов нужно на целую фигуру.",
            )
            .size(12.5)
            .color(theme::TEXT_DIM),
        );
        ui.add_space(4.0);
        egui::Grid::new("hits")
            .min_col_width(0.0)
            .spacing([3.0, 3.0])
            .show(ui, |ui| {
                ui.label("");
                for defender in Kind::ALL {
                    piece_icon(ui, &self.art, Color::Black, defender, 24.0);
                }
                ui.end_row();
                for attacker in Kind::ALL {
                    piece_icon(ui, &self.art, Color::White, attacker, 24.0);
                    for defender in Kind::ALL {
                        let dealt = attacker.stats().attack;
                        let hp = rogue_max_hp(defender);
                        let hits = (hp + dealt - 1) / dealt;
                        let (rect, response) =
                            ui.allocate_exact_size(vec2(32.0, 24.0), Sense::hover());
                        // Чем меньше ударов, тем горячее клетка.
                        ui.painter().rect_filled(
                            rect,
                            CornerRadius::same(3),
                            heat_color(2.0 / hits as f32),
                        );
                        ui.painter().text(
                            rect.center(),
                            Align2::CENTER_CENTER,
                            hits.to_string(),
                            FontId::new(13.0, theme::bold()),
                            theme::TEXT_BRIGHT,
                        );
                        let _ = response.on_hover_text(format!(
                            "{} бьёт {} на {dealt}: целую фигуру убивает за {hits} {}.",
                            attacker.title(),
                            defender.accusative(),
                            plural(hits, "удар", "удара", "ударов"),
                        ));
                    }
                    ui.end_row();
                }
            });
    }
}

impl eframe::App for BattleChessApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        // Время прошлого кадра — без ожидания в ограничителе.
        let work = frame
            .info()
            .cpu_usage
            .map(|cpu| (cpu - self.slept).max(0.0));
        self.limit_frame_rate();
        let now = ctx.input(|i| i.time);
        self.now = now;
        self.fps.record(now, work);
        self.sfx.enabled = self.settings.sound;
        self.sfx.volume = self.settings.volume;
        self.poll_net();
        // Меню и выбор колоды — вместо партии; партия на это время стоит.
        if self.screen != Screen::Game {
            // Редактор в браузере сохранил файл или вернулись в окно (например, из Блокнота с
            // balance.ini) — читаем баланс заново. Посреди партии не читаем: возьмёт новая партия.
            let focused = ctx.input(|i| i.viewport().focused.unwrap_or(true));
            if crate::editor::take_saved() || (focused && !self.window_focused) {
                self.reload_balance();
            }
            self.window_focused = focused;
            self.menu(ctx);
            self.paint_cursor(ctx);
            self.save_settings(ctx);
            return;
        }
        self.shortcuts(ctx, now);
        self.tick(now);

        let side_frame = egui::Frame::default()
            .fill(theme::PAGE)
            .inner_margin(Margin::same(12));
        egui::SidePanel::right("game")
            .resizable(false)
            .exact_width(320.0)
            .show_separator_line(false)
            .frame(side_frame)
            .show(ctx, |ui| self.game_panel(ui));
        egui::CentralPanel::default()
            .frame(
                egui::Frame::default()
                    .fill(theme::PAGE)
                    .inner_margin(Margin::same(12)),
            )
            .show(ctx, |ui| self.board_area(ui, now));

        let banner = self.ended_at.is_some_and(|t| now - t < BANNER_TIME);
        // Кадры рисуются, только когда что-то движется (анимации, подсказка удара),
        // — тогда с полной частотой. Счётчик FPS рисует кадры непрерывно, чтобы
        // показывать настоящую частоту.
        let stats = self
            .stats_for
            .is_some_and(|(_, since)| now - since < STATS_TIME)
            || self
                .deck_hover
                .is_some_and(|(_, since)| now - since < STATS_TIME);
        if self.anim.is_some()
            || !self.strikes.is_empty()
            || !self.meteors.is_empty()
            || !self.zaps.is_empty()
            || !self.meteor_waves.is_empty()
            || self.game.position().craters() != 0
            || !self.craters.is_empty()
            || !self.due_hits.is_empty()
            || !self.marches.is_empty()
            || !self.appears.is_empty()
            || !self.lavas.is_empty()
            || !self.effects.is_empty()
            || banner
            || stats
            || self.cards_moving()
            || self.card_rising(now)
            || self.settings.show_fps
            || self.hovered_target().is_some()
        {
            ctx.request_repaint();
        } else if self.computer_since.is_some() || self.idle_animation() {
            // Компьютер ждёт, особые фигуры и могилы движутся и в покое — кадр раз в 30 мс.
            ctx.request_repaint_after(Duration::from_millis(30));
        }
        self.paint_cursor(ctx);
        self.save_settings(ctx);
    }
}

impl BattleChessApp {
    /// В партии есть то, что движется и в покое: особые фигуры (огонь, молнии, вихрь, тень,
    /// яд) на доске, в запасе или на прилавке, могилы зомби, горящие кратеры.
    fn idle_animation(&self) -> bool {
        if self.screen != Screen::Game {
            return false;
        }
        let pos = self.game.position();
        pos.pieces()
            .any(|(_, p)| p.fire || p.spark || p.wind || p.shade || p.venom)
            || (0..64).any(|sq| pos.grave(sq).is_some())
            || pos.craters() != 0
            || pos.shop_open()
            || pos.rogue().is_some_and(|r| {
                pos.reserve(r.player)
                    .iter()
                    .any(|(stock, ..)| stock.power != Power::Plain)
            })
    }

    /// Без VSync не даём кадрам идти чаще выбранного предела.
    fn limit_frame_rate(&mut self) {
        let cap = match self.settings.frame_rate {
            FrameRate::Cap(fps) => Some(fps),
            // VSync выбран, но включится только после перезапуска — пока держим разумный предел.
            FrameRate::Vsync => Some(240),
            FrameRate::Unlimited => None,
        };
        self.slept = 0.0;
        let Some(fps) = cap.filter(|_| !self.vsync) else {
            return;
        };
        // Кадры идут по расписанию: если сон затянулся, следующий кадр будет раньше,
        // и в среднем частота точно равна пределу. После простоя расписание начинается заново.
        let interval = Duration::from_secs_f64(1.0 / f64::from(fps));
        let now = Instant::now();
        let target = match self.next_frame {
            Some(target) if target + interval > now => target,
            _ => now,
        };
        if let Some(rest) = target.checked_duration_since(now) {
            std::thread::sleep(rest);
            self.slept = rest.as_secs_f32();
        }
        self.next_frame = Some(target + interval);
    }

    fn save_settings(&mut self, ctx: &egui::Context) {
        // Пока тянут ползунок, не пишем на диск каждый кадр.
        if self.settings != self.saved && !ctx.input(|i| i.pointer.any_down()) {
            self.settings.save();
            self.saved = self.settings;
        }
    }

    /// Счётчик кадров в углу поля.
    fn paint_fps(&self, ctx: &egui::Context, corner: Pos2) {
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("fps"),
        ));
        let mode = if self.vsync { " · VSync" } else { "" };
        let text = format!(
            "{:.0} FPS · кадр {:.1} мс{mode}",
            self.fps.fps(),
            self.fps.cpu * 1000.0
        );
        let galley = painter.layout_no_wrap(text, FontId::proportional(13.0), theme::TEXT_BRIGHT);
        let rect = Rect::from_min_size(corner, galley.size() + vec2(12.0, 6.0));
        painter.rect_filled(rect, CornerRadius::same(4), rgba(0, 0, 0, 0.6));
        painter.galley(rect.min + vec2(6.0, 3.0), galley, theme::TEXT_BRIGHT);
    }

    /// Громкость и выключатель звука; отпущенный ползунок проигрывает пример.
    fn volume_controls(&mut self, ui: &mut egui::Ui) {
        ui.checkbox(&mut self.settings.sound, "Звук включён");
        ui.add_enabled_ui(self.settings.sound, |ui| {
            // Ползунок во всю ширину, справа — поле с числом.
            ui.spacing_mut().slider_width = (ui.available_width() - 64.0).max(80.0);
            let slider = ui
                .add(egui::Slider::new(&mut self.settings.volume, 0..=100).suffix("%"))
                .on_hover_text("Громкость");
            if slider.drag_stopped() || (slider.changed() && !slider.dragged()) {
                self.sfx.enabled = self.settings.sound;
                self.sfx.volume = self.settings.volume;
                self.sfx.play(Sound::Move);
            }
        });
    }
}

// ----- Рисование -----

fn hex(rgb: u32) -> Color32 {
    Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

/// Фон под доской, как в Into the Breach: тёмно-синий, к низу темнее, по краям тень.
/// Целый масштаб текстур по размеру окна при зуме 100%. Колёсико и ползунок
/// масштаба доски сюда не входят: смена этого числа заново заливает весь арт.
fn texture_scale_for(fit_at_100: f32) -> u32 {
    let whole = (fit_at_100 + 0.01 * fit_at_100).floor();
    let physical = if whole >= 1.0 && whole >= 0.92 * fit_at_100 {
        whole
    } else {
        fit_at_100.max(0.25)
    };
    physical.ceil().clamp(1.0, 8.0) as u32
}

fn paint_backdrop(painter: &Painter, rect: Rect) {
    const STEPS: u32 = 12;
    let (top, bottom) = ([0x20, 0x24, 0x2f], [0x0f, 0x11, 0x17]);
    let mut mesh = Mesh::default();
    for row in 0..=STEPS {
        for col in 0..=STEPS {
            let (u, v) = (col as f32 / STEPS as f32, row as f32 / STEPS as f32);
            let light = (1.15 - ((u - 0.5) / 0.62).hypot((v - 0.45) / 0.7) * 0.45).clamp(0.6, 1.1);
            let channel = |k: usize| {
                ((top[k] as f32 * (1.0 - v) + bottom[k] as f32 * v) * light).min(255.0) as u8
            };
            let color = Color32::from_rgb(channel(0), channel(1), channel(2));
            mesh.colored_vertex(rect.lerp_inside(vec2(u, v)), color);
        }
    }
    let width = STEPS + 1;
    for row in 0..STEPS {
        for col in 0..STEPS {
            let i = row * width + col;
            mesh.add_triangle(i, i + 1, i + width);
            mesh.add_triangle(i + 1, i + width + 1, i + width);
        }
    }
    painter.add(Shape::mesh(mesh));
}

/// Цвета пиксельных чисел над доской — палитра Into the Breach.
const PIXEL_WHITE: Color32 = Color32::from_rgb(0xf4, 0xf1, 0xdf);
const PIXEL_STEEL: Color32 = Color32::from_rgb(0xa6, 0xc8, 0xe0);
const PIXEL_GOLD: Color32 = Color32::from_rgb(0xff, 0xd2, 0x4a);
const PIXEL_RED: Color32 = Color32::from_rgb(0xef, 0x4b, 0x3c);

const PIXEL_GREEN: Color32 = Color32::from_rgb(0x53, 0xd0, 0x4a);

/// Знаки одного цвета.
fn paint(glyphs: &[Glyph], color: Color32) -> Vec<(Glyph, Color32)> {
    glyphs.iter().map(|&g| (g, color)).collect()
}

/// Ход выпада: быстро до касания, затем плавный возврат (0 — на месте, 1 — в точке удара).
fn lunge_reach(u: f64) -> f32 {
    if u < LUNGE_IMPACT {
        ease_out((u / LUNGE_IMPACT) as f32)
    } else {
        1.0 - ease_in_out(((u - LUNGE_IMPACT) / (1.0 - LUNGE_IMPACT)) as f32)
    }
}

/// Здоровье выше 70% — зелёное, от 30 до 70% — жёлтое, ниже 30% — красное.
fn hp_color(frac: f32) -> Color32 {
    if frac > 0.7 {
        PIXEL_GREEN
    } else if frac >= 0.3 {
        Color32::from_rgb(0xf2, 0xc5, 0x3d)
    } else {
        PIXEL_RED
    }
}

fn heat_color(t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let mix = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t) as u8;
    Color32::from_rgb(mix(0x3a, 0xb8), mix(0x38, 0x45), mix(0x35, 0x2a))
}

/// Полоска здоровья в карточке.
fn hp_meter(ui: &mut egui::Ui, hp: i32, max: i32) {
    let (rect, _) =
        ui.allocate_exact_size(vec2(ui.available_width().min(200.0), 8.0), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(3), rgba(0, 0, 0, 0.45));
    let x = |hp: i32| rect.min.x + rect.width() * (hp.max(0) as f32 / max as f32).clamp(0.0, 1.0);
    let frac = (hp as f32 / max as f32).clamp(0.0, 1.0);
    let filled = Rect::from_min_max(rect.min, pos2(x(hp), rect.max.y));
    painter.rect_filled(filled, CornerRadius::same(3), hp_color(frac));
}

/// Значок фигуры, вписанный в квадрат `size`.
fn piece_icon(ui: &mut egui::Ui, art: &Art, color: Color, kind: Kind, size: f32) {
    let (texture, pixels) = art.icon(ui.ctx(), color, kind);
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    let image = pixels * (size / pixels.max_elem());
    let image = Rect::from_center_size(rect.center(), image);
    ui.painter().image(texture, image, UV, Color32::WHITE);
}

fn card<R>(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::default()
        .fill(theme::PANEL)
        .corner_radius(CornerRadius::same(4))
        .inner_margin(Margin::same(10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.vertical(add_contents).inner
        })
        .inner
}

fn section_title(ui: &mut egui::Ui, text: &str) {
    ui.label(
        RichText::new(text)
            .size(15.5)
            .family(theme::bold())
            .color(theme::TEXT_BRIGHT),
    );
}

fn option_label(ui: &mut egui::Ui, text: &str) {
    ui.add_space(4.0);
    ui.label(RichText::new(text).size(12.5).color(theme::TEXT_DIM));
}

/// Правила rogue-like (с 0.108 — основной режим) для экрана «Правила»; числа — из баланса.
fn rules() -> Vec<String> {
    let rule = |rule: Rule| balance::balance().rule(rule);
    vec![
        format!(
            "Ход — до {} ходов разными фигурами (воздушная ходит дважды за один); сколько \
             осталось — над кнопкой «Конец хода» (или Space). Закончить ход можно и не сходив.",
            rule(Rule::RogueMoves)
        ),
        "Бить ходом нельзя: фигура ходит только на пустую клетку. Фигуры бьют сами — в конце \
         хода своей стороны: пешка — по диагонали вперёд, конь — буквой «Г», король — вокруг, \
         слон, ладья и ферзь — первую фигуру на своих линиях."
            .to_owned(),
        "Удар снимает с цели столько здоровья, сколько у атакующего атаки, — без случайностей и \
         без ответа."
            .to_owned(),
        "Урон ваших фигур — атака × мульти (сначала 1; сейчас — над золотом). Гербы мульти \
         применяются по порядку покупки: герб урона прибавляет к мульти своё число, герб урона \
         короля умножает его на своё число за каждый удар вашего короля в этом ходу. Урон и король, \
         король ударил дважды: 10 × (1 + 4) × 1.5 × 1.5; король и урон: 10 × (1 × 1.5 × 1.5 + 4). \
         Прибавки от ударов и временных эффектов действуют до конца хода. Предела урона нет. \
         Взрыв огненной фигуры и метеорит мага бьют уроном самой фигуры — атака × мульти, \
         горящий кратер — уроном упавшего метеорита."
            .to_owned(),
        "Фигуры соперника выходят волнами на его крайнюю горизонталь и в конце каждого его хода \
         шагают вперёд. Волна дошла до вашей первой горизонтали или погиб король — поражение."
            .to_owned(),
        format!(
            "Уровень — несколько волн; отбили все — магазин и расстановка, потом следующий \
             уровень. Всего уровней — {}. На первом у фигур волн здоровья на {} меньше, на              каждом следующем — в {} раза больше, чем на прошлом.",
            rule(Rule::RogueLevels),
            rule(Rule::RogueFirstLevelHpCut),
            game::mult_text(game::hundredths(Rule::RogueLevelHpPercent))
        ),
        format!(
            "Золото — за каждую побеждённую фигуру. В магазине — только сундуки: фигур (в запас), \
             гербов (до конца забега), карт (в руку) и медалей. «Обновить» ({}) кладёт новые \
             сундуки. Магазин можно свернуть — «Магазин» наверху разворачивает его.",
            rule(Rule::RogueRerollCost)
        ),
        format!(
            "Сундук медалей: виды фигур на выбор; выбранный до конца забега получает мульти +{} — \
             раньше всех гербов. Медали — в правом верхнем углу.",
            game::mult_text(game::hundredths(Rule::RogueMedalMultPercent))
        ),
        format!(
            "Фигуру из запаса выставляют вместо хода — {} за ход, на две первые горизонтали \
             (пешку — на вторую) и пока ваших фигур на доске меньше {}.",
            match rule(Rule::RogueDropsPerTurn) {
                1 => "одну".to_owned(),
                n => format!("до {n}"),
            },
            rule(Rule::RogueMaxPieces)
        ),
        format!(
            "Сундуки магазина: одно из {} на выбор. Карта ложится в руку (до {}, слева вверху); \
             щёлкните по ней — карты на фигуры (силы, ранг, зеркало) попросят щёлкнуть по своей \
             фигуре, остальные сыграются сразу. Карты играют в раунде, в расстановке и в \
             магазине (там щелчок по карте — «Сыграть» или «Продать»). Наведите курсор на карту \
             — появится, что она делает.",
            rule(Rule::RogueChestChoices),
            rule(Rule::RogueHandSize)
        ),
        format!(
            "Продажа в магазине: фигуру запаса перетащите в «Продажу» (за цену обычной такой же, \
             золотую — дороже), герб — щёлкните по нему в колонке гербов слева и нажмите «Продать» \
             ({}), карту — щёлкните по ней и нажмите «Продать» ({}).",
            rule(Rule::RogueCrestSellPrice),
            rule(Rule::RogueCardSellPrice)
        ),
        "Расстановка: выжившие фигуры возвращаются на клетки начала уровня здоровыми; их можно \
         передвигать и менять местами на двух первых горизонталях."
            .to_owned(),
        format!(
            "Особые фигуры: огненные взрываются, электрические бьют молнией, воздушные ходят \
             дважды, золотые приносят больше золота, теневые ходят сквозь фигуры, ледяные \
             замораживают ударивших, варвары бьют снова после убийства, маги шлют метеорит раз \
             в {}.",
            match rule(Rule::RogueWizardEvery) {
                1 => "ход".to_owned(),
                n @ 2..=4 => format!("{n} хода"),
                n => format!("{n} ходов"),
            }
        ),
        "Замёрзшая фигура пропускает ход своей стороны — не ходит и не бьёт.".to_owned(),
    ]
}
/// Правила списком — на экране «Правила» главного меню.
fn rules_section(ui: &mut egui::Ui) {
    for rule in rules() {
        ui.label(format!("• {rule}"));
        ui.add_space(3.0);
    }
}

/// Полное здоровье фигуры вида `kind` в rogue-like (у коня — своё, `rogue_knight_hp`): для
/// таблиц экрана «Правила», какая бы партия ни была начата.
fn rogue_max_hp(kind: Kind) -> i32 {
    if kind == Kind::Knight {
        game::rogue_knight_hp()
    } else {
        kind.stats().max_hp
    }
}

/// Действия по ходам: подряд идущие действия одной стороны, без отметок конца хода.
fn group_turns(history: &[Record]) -> Vec<Vec<&Record>> {
    let mut turns: Vec<Vec<&Record>> = Vec::new();
    let mut last = None;
    for record in history {
        if last != Some(record.color()) {
            turns.push(Vec::new());
            last = Some(record.color());
        }
        // Конец хода — в списке, только если фигуры ударили.
        let quiet_end = matches!(&record.event, Event::EndTurn(_, turn) if turn.is_quiet());
        if !quiet_end && let Some(turn) = turns.last_mut() {
            turn.push(record);
        }
    }
    turns
}

/// Ход в списке ходов: «Кb1–c3», карта — «Огненная Волна d4 −900», удары в конце хода —
/// «Кc3⚔d5, Сc4×e6 −1000», через точку.
fn turn_job(turn: &[&Record], highlighted: bool, wrap: f32) -> LayoutJob {
    let color = if highlighted {
        theme::TEXT_BRIGHT
    } else {
        theme::TEXT
    };
    let font = FontId::proportional(14.5);
    let small = FontId::proportional(13.0);
    let mut job = LayoutJob::default();
    job.wrap.max_width = wrap;
    for (i, record) in turn.iter().enumerate() {
        if i > 0 {
            job.append(
                " · ",
                0.0,
                TextFormat::simple(font.clone(), theme::TEXT_DIM),
            );
        }
        let notation = record.notation();
        let text_color = match record.event {
            Event::Card(_) => theme::ORANGE,
            Event::EndTurn(..) => theme::SALMON,
            Event::Rogue(_) => theme::GOLD,
            Event::Move(_) => color,
        };
        job.append(
            &notation.text,
            0.0,
            TextFormat::simple(font.clone(), text_color),
        );
        if let Some(dealt) = notation.damage {
            job.append(
                &format!(" −{}", shown(dealt)),
                0.0,
                TextFormat::simple(small.clone(), theme::RED),
            );
        }

        job.append(
            notation.suffix,
            0.0,
            TextFormat::simple(font.clone(), color),
        );
    }
    job
}
fn result_text(status: Status, pos: &Position) -> (&'static str, String) {
    match (status, pos.rogue()) {
        (Status::Victory { winner }, Some(r)) if winner == r.player => {
            ("Победа", format!("Все {} уровней пройдены", r.levels))
        }
        (Status::Victory { .. }, Some(_)) if pos.breached() => (
            "Поражение",
            "Волна дошла до вашей первой горизонтали".to_owned(),
        ),
        (Status::Victory { .. }, Some(r)) => (
            "Поражение",
            format!(
                "Волны одолели на уровне {} — волна {} из {}",
                r.level, r.wave, r.waves
            ),
        ),
        _ => result_text_plain(status),
    }
}

fn result_text_plain(status: Status) -> (&'static str, String) {
    match status {
        Status::Victory { winner } => (
            "Победа",
            format!(
                "Победили {} — у {} не осталось фигур",
                winner.name().to_lowercase(),
                winner.opponent().genitive()
            ),
        ),
        Status::Encircled { winner } => (
            "Победа",
            format!(
                "Победили {} — {} окружены и сломлены",
                winner.name().to_lowercase(),
                winner.opponent().name().to_lowercase()
            ),
        ),
        Status::FiftyMoves => ("Ничья", "50 ходов без ударов и ходов пешками".to_owned()),
        Status::Repetition => ("Ничья", "Позиция повторилась трижды".to_owned()),
        Status::Ongoing => ("", String::new()),
    }
}

fn score_text(status: Status) -> &'static str {
    match status {
        Status::Victory {
            winner: Color::White,
        }
        | Status::Encircled {
            winner: Color::White,
        } => "1–0",
        Status::Victory {
            winner: Color::Black,
        }
        | Status::Encircled {
            winner: Color::Black,
        } => "0–1",
        Status::Ongoing => "",
        _ => "½–½",
    }
}

/// «удар», «удара», «ударов».
fn plural(n: i32, one: &'static str, few: &'static str, many: &'static str) -> &'static str {
    let n = n.unsigned_abs() % 100;
    if (11..=14).contains(&n) {
        return many;
    }
    match n % 10 {
        1 => one,
        2..=4 => few,
        _ => many,
    }
}

/// Оттенок рамки клетки-цели: она «дышит» от `TARGET_DIM` до полной яркости.
fn target_tint(now: f64) -> Color32 {
    let wave = 0.5 + 0.5 * (now * std::f64::consts::TAU / TARGET_PULSE).cos() as f32;
    let k = TARGET_DIM + (1.0 - TARGET_DIM) * wave;
    Color32::from_white_alpha((k * 255.0).round() as u8)
}

fn center(sq: Square) -> Vec2 {
    vec2(
        game::file_of(sq) as f32 + 0.5,
        game::rank_of(sq) as f32 + 0.5,
    )
}

/// Плавность движения как у chessground (lichess).
fn ease_in_out(t: f32) -> f32 {
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        let u = t - 1.0;
        1.0 + 4.0 * u * u * u
    }
}

/// Быстрый выход с перелётом чуть дальше цели и возвратом.
fn ease_out_back(t: f32) -> f32 {
    let k = 1.7;
    let u = t - 1.0;
    1.0 + (k + 1.0) * u * u * u + k * u * u
}

fn ease_out(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn board_points_map_back_to_their_squares() {
        for flipped in [false, true] {
            let board = BoardGeom {
                area: Rect::from_min_size(Pos2::ZERO, vec2(2000.0, 2000.0)),
                origin: pos2(100.0, 150.0),
                scale: 2.4,
                texture_scale: 3,
                ppp: 1.0,
                flipped,
            };
            for sq in 0..64 {
                let ground = board.ground(sq);
                assert_eq!(board.square_at(ground), Some(sq));
                // Углы ромба клетки чуть внутри — та же клетка.
                let tile = board.tile_rect(sq);
                for corner in [
                    tile.left_center(),
                    tile.right_center(),
                    tile.center_top(),
                    tile.center_bottom(),
                ] {
                    let inside = corner + (ground - corner) * 0.1;
                    assert_eq!(board.square_at(inside), Some(sq));
                }
                assert!((board.to_board(ground) - center(sq)).length() < 1e-4);
            }
            // Вне ромбов — ни одной клетки.
            assert_eq!(board.square_at(board.origin), None);
        }
    }

    #[test]
    fn own_pieces_stand_at_the_lower_left() {
        let white = BoardGeom {
            area: Rect::ZERO,
            origin: Pos2::ZERO,
            scale: 1.0,
            texture_scale: 1,
            ppp: 1.0,
            flipped: false,
        };
        let a1 = white.ground(game::parse_square("a1").unwrap());
        let h1 = white.ground(game::parse_square("h1").unwrap());
        let a8 = white.ground(game::parse_square("a8").unwrap());
        // a1 — левый угол, h1 — нижний, a8 — верхний.
        assert!(a1.x < h1.x && a1.y < h1.y && a8.y < a1.y);
        let black = BoardGeom {
            flipped: true,
            ..white
        };
        assert_eq!(black.ground(game::parse_square("h8").unwrap()), a1);
        // Ближние к зрителю клетки рисуются позже.
        assert!(
            white.depth(center(game::parse_square("h1").unwrap()))
                > white.depth(center(game::parse_square("a8").unwrap()))
        );
    }
}
