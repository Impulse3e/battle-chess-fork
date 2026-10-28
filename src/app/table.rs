//! Стол, как в арт-проекте (`tools/breach_cards.py`, `table_preview`, «вариант 12»): своя
//! колода в левом нижнем углу, рука веером под правым краем доски; колода и рука соперника
//! — то же, повёрнутое на 180° вокруг центра доски; кнопка «Конец хода» — справа по
//! центру. Каждый ход сторона берёт карту из колоды и может разыграть до трёх карт.
//!
//! Карту берут мышью, как в Hearthstone: она примагничивается серединой к курсору и
//! тянется за ним, покачиваясь от скорости; отпущенная над рукой — возвращается, в другом
//! месте — разыгрывается и сгорает. Карта с выбором фигуры (огненная волна, обмен) после
//! этого встаёт над рукой, и от короля к курсору тянется стрелка, как в Slay the Spire
//! (`tools/breach_arrow.py`); щелчок по своей фигуре применяет карту, обмену нужны две.
//! Карту на поле (буря, лавина, ледяной барьер) направляют на любую клетку: подсвечены
//! клетки, на которые ляжет лёд (`tools/breach_freeze.py`).

use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontFamily, FontId, Painter, Pos2, Rect, Stroke,
    StrokeKind, Vec2, epaint::TextShape, vec2,
};

use super::{
    BattleChessApp, BoardGeom, Effect, Figure, Mark, PIXEL_WHITE, STATS_TIME, Sink, UV,
    ease_in_out, ease_out, ease_out_back, paint,
};
use crate::assets::{
    ANCHOR, ARROW_HEAD, ARROW_SPACING, Aim, ArrowPart, BIG_CHEST, BIG_WAVE_CENTER, BIG_WAVE_FRAMES,
    BLESS_FRAME_TIME, BLESS_FRAMES, BURN_CARD_AT, BURN_FRAME, BURN_FRAME_TIME, BURN_FRAMES, BUTTON,
    Button, CARD, CHEST, COIN, COIN_SMALL, COST_TINT, CREST, CardPic, DARK_LOOP, ELECTRIC_ANCHOR,
    ELECTRIC_FRAME_TIME, ELECTRIC_LOOP, EMBLEM_ZOOM, FREEZE_CENTER, FREEZE_CLINK, FREEZE_DELAY,
    FREEZE_FRAME_TIME, FREEZE_FRAMES, FrameSprite, GLYPH_HEIGHT, Glyph, LAVA_FRAME_TIME, LAVA_HOLD,
    MEDAL, POINTS_TINT, PORTAL_CENTER, PORTAL_FRAME, PORTAL_FRAME_TIME, PORTAL_FRAMES,
    PORTAL_SCALE, PORTAL_SINK, POWER_CARD, SHADOW_SHIFT, SPRITE, Sprite, TARGET_PAD, TILE,
    Textures, WAVE_CENTER, WAVE_FRAME, WAVE_FRAME_TIME, WAVE_FRAMES, WIND_LOOP, cost_corner,
};
use crate::game::{
    Area, Card, CardKind, CardPlay, ChestState, Color, Crest, Event, Family, Heal, Kind, Piece,
    Points, Power, Square, Status, Stock, Ware, WaveHit, card_sell_price, crest_sell_price,
    file_of, medal_description, medal_title, mult_text, rank_of,
};
use crate::sound::Sound;
use crate::theme;

/// Центр верхней грани доски, её левый и правый углы (правый — низ бока), пиксели арта.
const MID: Vec2 = vec2(224.0, 168.0);
const LEFT: Vec2 = vec2(0.0, 168.0);
const CORNER: Vec2 = vec2(448.0, 199.0);
/// Своя колода — левый верхний угол верхней карты, в левом нижнем углу стола; рука —
/// середина средней карты.
const DECK_AT: Vec2 = vec2(LEFT.x - 76.0, LEFT.y + 152.0);
const HAND_AT: Vec2 = vec2(CORNER.x - 24.0, CORNER.y + 135.0);
/// Кнопка «Конец хода» — левый верхний угол; справа по центру стола, как в арт-проекте.
const BUTTON_AT: Vec2 = vec2(472.0, 161.0);
/// Полная стопка (12 торцов): по ней колода соперника отражается через центр доски.
const DECK_FULL: Vec2 = vec2(54.0, 85.0);
/// Сколько торцов видно под верхней картой полной стопки.
const DECK_EDGES: u8 = 12;
/// Веер: карты на дуге радиуса `RADIUS`, через `STEP` градусов; при большой руке
/// шаг уменьшается, чтобы веер не разъезжался.
const RADIUS: f32 = 220.0;
const STEP: f32 = 8.5;
const SPREAD: f32 = 42.5;
/// На сколько приподнимается карта под курсором — та, что можно сыграть, и остальные.
const LIFT: f32 = 32.0;
const LIFT_IDLE: f32 = 20.0;
/// За столько секунд карта под курсором выезжает вверх.
const RISE_TIME: f64 = 0.2;
/// Карта под курсором крупнее втрое — как в Hearthstone, чтобы читался текст (карты
/// нарисованы вдвое детальнее, и крупная карта остаётся резкой).
const ZOOM: f32 = 3.0;
/// Карта из колоды долетает до руки.
const DRAW_TIME: f64 = 0.35;
/// Добор ждёт, пока доедет походившая фигура.
pub const DRAW_DELAY: f64 = 0.2;
/// Несколько карт подряд вылетают из колоды с таким промежутком.
const DRAW_GAP: f64 = 0.15;
/// Самый крупный кегль описания на лице карты, пиксели карты.
const DESCRIPTION_FONT: f32 = 5.0;
/// Свет исцеления доходит до соседних клеток на 12-м кадре (столбы света в breach_bless.py).
const HEAL_IMPACT: f64 = 12.0 * BLESS_FRAME_TIME;
/// Счётчики очков у колоды: отступ от колоды и между венком и черепом, пиксели арта.
const POINTS_GAP: f32 = 8.0;
const POINTS_SPACING: f32 = 6.0;
/// Число очков над знаком — вдвое крупнее цифры цены на карте, с отступом от знака.
const POINTS_FONT: f32 = 2.0;
const POINTS_RAISE: f32 = 4.0;
/// Мульти (с 0.119): цвет чисел, отступ над числом золота и строка числа на гербе от его верха,
/// пиксели арта.
const MULT_RED: Color32 = Color32::from_rgb(0xff, 0x5a, 0x4a);
const MULT_GAP: f32 = 6.0;
const BADGE_TOP: f32 = 26.0;
/// Взятая карта догоняет курсор: за 1/`FOLLOW` секунды проходит две трети пути —
/// середина карты за доли секунды «прилипает» к курсору и дальше идёт за ним с
/// лёгкой задержкой.
const FOLLOW: f32 = 28.0;
/// Покачивание взятой карты: наклон по часовой при движении вправо — градусов на
/// пиксель арта в секунду, предел и плавность.
const SWAY: f32 = 0.06;
const SWAY_MAX: f32 = 16.0;
const SWAY_EASE: f32 = 12.0;
/// Взятая карта поднята над столом — тень отходит дальше.
const HELD_SHADOW: Vec2 = vec2(6.0, 8.0);
/// Отпущенная над рукой карта возвращается на место.
const RETURN_TIME: f64 = 0.22;
/// Зона руки шире самих карт на столько пикселей арта.
const HAND_ZONE_MARGIN: f32 = 10.0;
/// Кромка огненной волны доходит до соседних клеток — тогда цифры урона.
const WAVE_IMPACT: f64 = 0.27;
/// У большой волны крестом — когда кромка прошла середину креста.
const BIG_WAVE_IMPACT: f64 = 0.36;
/// Снег без замёрзших фигур (после бури и лавины, растаявшей зоны) и лёд оттаявших фигур
/// тают за столько секунд.
const MELT_TIME: f64 = 0.4;
/// Наконечник стрелки смотрит в фигуру чуть выше центра клетки.
const AIM_RISE: f32 = 14.0;
/// Стрелка начинается чуть выше макушки короля (или первой фигуры обмена), пиксели арта.
const KING_CROWN: f32 = 2.0;
/// За столько секунд гаснет текст сгорающей карты.
const BURN_TEXT_TIME: f64 = 0.5;
/// Табличка с числом карт: отступ от колоды и откуда она выезжает, пиксели шрифта.
const DECK_PLATE_GAP: f32 = 3.0;
const DECK_PLATE_SLIDE: f32 = 4.0;
/// Значок карты на табличке — цвета огней белых и чёрных фигур (`GLOW` в breach_art.py).
const DECK_ICON: [Color32; 2] = [
    Color32::from_rgb(0x57, 0xd9, 0xff),
    Color32::from_rgb(0xff, 0x8a, 0x2e),
];

/// Поле стола вокруг картинки доски: с ним вместе доска вписывается в окно. Слева —
/// веер руки соперника (до −91 с тенью), справа — своя рука, сверху — колода соперника
/// (−69), снизу — своя колода с тенью (408). 648×480: в развёрнутом окне 1920×1080 с обеими
/// панелями — ровно вдвое (1296×960).
#[allow(dead_code)] // раскладка стола с картами (тесты)
pub const TABLE_MIN: Vec2 = vec2(-100.0, -70.0);
#[allow(dead_code)]
pub const TABLE_MAX: Vec2 = vec2(548.0, 410.0);
/// Что вписывается в окно с 0.48, когда карт нет: доска с подписями, кнопка «Конец хода» и
/// полосы запаса у углов доски (своя — слева внизу, соперника — справа вверху).
pub const VIEW_MIN: Vec2 = vec2(-64.0, -34.0);
pub const VIEW_MAX: Vec2 = vec2(530.0, 370.0);
/// В rogue-like (с 0.114) слева вписывается ещё колонка гербов (`CRESTS_AT`).
pub const ROGUE_VIEW_MIN: Vec2 = vec2(-178.0, -34.0);

/// Место за столом: своё — внизу справа, соперника — вверху слева.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Seat {
    Near,
    Far,
}

/// Карта в руке: середина и поворот по часовой стрелке, в градусах.
#[derive(Clone, Copy)]
struct Slot {
    center: Vec2,
    degrees: f32,
}

/// Карта, которую держат мышью.
pub struct Held {
    seat: Seat,
    /// Место карты в руке — туда она вернётся.
    index: usize,
    kind: CardKind,
    /// Середина карты и наклон; курсор — в пикселях арта.
    at: Vec2,
    tilt: f32,
    pointer: Vec2,
    last: f64,
}

/// Карта с выбором фигуры ждёт цель: стоит над рукой, к курсору тянется стрелка.
pub struct Aiming {
    seat: Seat,
    index: usize,
    kind: CardKind,
    /// Первая выбранная фигура обмена: вторую выбирают из тех, с кем её можно поменять.
    first: Option<Square>,
    /// Свои фигуры, которые можно выбрать сейчас: пока карта ждёт, позиция не меняется.
    targets: Vec<Square>,
}

/// Карта, летящая обратно в руку.
pub struct Returning {
    seat: Seat,
    index: usize,
    kind: CardKind,
    from: Vec2,
    tilt: f32,
    start: f64,
}

/// Сгорающая карта.
pub struct Burn {
    kind: CardKind,
    color: Color,
    /// Середина карты, пиксели арта.
    center: Vec2,
    start: f64,
}

/// Карта, летящая из колоды в руку.
pub struct Draw {
    color: Color,
    start: f64,
}

/// Огненная волна вокруг фигуры.
pub struct Wave {
    center: Square,
    hits: Vec<WaveHit>,
    start: f64,
    impacted: bool,
    /// Большая волна крестом (герб двух огоньков, с 0.87).
    wide: bool,
}

impl Wave {
    /// Огненная волна вокруг `center`, начинается в `start` (может быть впереди).
    pub fn new(center: Square, hits: Vec<WaveHit>, start: f64) -> Wave {
        Wave {
            center,
            hits,
            start,
            impacted: false,
            wide: false,
        }
    }

    /// Большая волна крестом.
    pub fn wide(self, wide: bool) -> Wave {
        Wave { wide, ..self }
    }

    /// Когда всплывают цифры урона: большая волна доходит до дальних клеток позже.
    fn impact(&self) -> f64 {
        if self.wide {
            BIG_WAVE_IMPACT
        } else {
            WAVE_IMPACT
        }
    }

    /// Сколько длится.
    fn length(&self) -> f64 {
        let frames = if self.wide {
            BIG_WAVE_FRAMES
        } else {
            WAVE_FRAMES
        };
        frames as f64 * WAVE_FRAME_TIME
    }
}

/// Исцеление (`tools/breach_bless.py`): слон светится и раскрывает крылья, по соседним
/// клеткам бежит кольцо света — тогда над вылеченными фигурами всплывают зелёные цифры.
pub struct Bless {
    center: Square,
    color: Color,
    heals: Vec<Heal>,
    start: f64,
    impacted: bool,
}

/// Обмен (`tools/breach_portal.py`): под обеими фигурами раскрываются порталы, фигуры
/// проваливаются, потом поднимаются каждая на клетке другой.
pub struct Portal {
    squares: [Square; 2],
    start: f64,
    sounded: bool,
    /// Фигуры клеток до обмена (теневая, ушедшая порталом, с 0.88); `None` — берутся из позиции.
    pieces: Option<[Piece; 2]>,
}

impl Portal {
    /// Обмен фигур `pieces` (как стояли до него) на клетках `squares`.
    pub(super) fn swap(squares: [Square; 2], pieces: [Piece; 2], start: f64) -> Portal {
        Portal {
            squares,
            start,
            sounded: false,
            pieces: Some(pieces),
        }
    }

    /// Сколько длится: исчезновение и появление.
    pub(super) fn length() -> f64 {
        2.0 * PORTAL_FRAMES as f64 * PORTAL_FRAME_TIME
    }

    /// Что сейчас: `appear` — фигуры поднимаются (иначе проваливаются) — и кадр;
    /// `None` — обмен кончился.
    fn phase(&self, now: f64) -> Option<(bool, usize)> {
        let frame = ((now - self.start).max(0.0) / PORTAL_FRAME_TIME) as usize;
        match frame {
            f if f < PORTAL_FRAMES => Some((false, f)),
            f if f < 2 * PORTAL_FRAMES => Some((true, f - PORTAL_FRAMES)),
            _ => None,
        }
    }
}

/// Заморозка (`tools/breach_freeze.py`): от клетки прицела по клеткам карты расходится
/// лёд — наметает снег, на замёрзших фигурах поднимается иней и вырастает глыба. Фигура,
/// вставшая в ледяную зону, замерзает так же, только снег под ней уже лежит.
pub struct Freeze {
    /// Клетки, где наметает снег, и задержка каждой, кадров.
    tiles: Vec<(Square, usize)>,
    /// Клетки замёрзших фигур и задержка каждой.
    pieces: Vec<(Square, usize)>,
    start: f64,
    sounded: bool,
}

impl Freeze {
    /// Лёд расходится от `origin`: на клетке дальше на одну — на `FREEZE_DELAY` кадров позже.
    pub(super) fn new(origin: Square, tiles: &[Square], pieces: &[Square], start: f64) -> Freeze {
        let delay = |sq: Square| {
            let (df, dr) = (file_of(sq) - file_of(origin), rank_of(sq) - rank_of(origin));
            df.unsigned_abs().max(dr.unsigned_abs()) as usize * FREEZE_DELAY
        };
        Freeze {
            tiles: tiles.iter().map(|&sq| (sq, delay(sq))).collect(),
            pieces: pieces.iter().map(|&sq| (sq, delay(sq))).collect(),
            start,
            sounded: false,
        }
    }

    /// Кадр на клетке `sq` из списка `list`: `Some(None)` — лёд ещё не дошёл, `None` —
    /// клетки в списке нет.
    fn frame(&self, list: &[(Square, usize)], sq: Square, now: f64) -> Option<Option<usize>> {
        let &(_, delay) = list.iter().find(|&&(s, _)| s == sq)?;
        let f = ((now - self.start) / FREEZE_FRAME_TIME).floor() - delay as f64;
        Some((f >= 0.0).then_some(f as usize))
    }

    /// Когда на клетке с задержкой `delay` лёд встал целиком.
    fn settled(&self, delay: usize) -> f64 {
        self.start + (delay + FREEZE_FRAMES) as f64 * FREEZE_FRAME_TIME
    }

    /// Когда лёд встал на всех клетках.
    fn done(&self) -> f64 {
        let last = self.tiles.iter().chain(&self.pieces).map(|&(_, d)| d).max();
        self.settled(last.unwrap_or(0))
    }
}

impl Seat {
    fn sign(self) -> f32 {
        match self {
            Seat::Near => 1.0,
            Seat::Far => -1.0,
        }
    }

    fn hand_at(self) -> Vec2 {
        match self {
            Seat::Near => HAND_AT,
            Seat::Far => MID * 2.0 - HAND_AT,
        }
    }

    fn deck_at(self) -> Vec2 {
        match self {
            Seat::Near => DECK_AT,
            Seat::Far => MID * 2.0 - (DECK_AT + DECK_FULL),
        }
    }

    /// Карта `k` из `n` в веере; приподнятая карта стоит прямо.
    fn slot(self, k: usize, n: usize, lift: f32) -> Slot {
        let sign = self.sign();
        let step = if n > 1 {
            STEP.min(SPREAD / (n - 1) as f32)
        } else {
            STEP
        };
        let angle = (k as f32 - (n as f32 - 1.0) / 2.0) * step;
        let (sin, cos) = angle.to_radians().sin_cos();
        let base = self.hand_at();
        let pivot = base.y + sign * RADIUS;
        Slot {
            center: vec2(base.x + sin * RADIUS, pivot - sign * (cos * RADIUS + lift)),
            degrees: if lift > 0.0 { 0.0 } else { sign * angle },
        }
    }

    /// Зона руки из `n` карт: над ней отпущенная карта возвращается в руку. От веера
    /// с приподнятыми картами — до края стола за рукой.
    fn hand_zone(self, n: usize) -> Rect {
        let half = CARD / 2.0 + Vec2::splat(HAND_ZONE_MARGIN);
        let mut zone = Rect::NOTHING;
        for k in 0..n.max(1) {
            for lift in [0.0, LIFT] {
                let c = self.slot(k, n.max(1), lift).center;
                zone = zone.union(Rect::from_center_size(c.to_pos2(), half * 2.0));
            }
        }
        match self {
            Seat::Near => zone.with_max_y(f32::INFINITY),
            Seat::Far => zone.with_min_y(f32::NEG_INFINITY),
        }
    }
}

/// Кнопка «Конец хода» на столе, пиксели арта.
fn button_rect() -> Rect {
    Rect::from_min_size(BUTTON_AT.to_pos2(), BUTTON)
}

/// Запас взятых фигур («шведка», с 0.43): своя полоса — внизу слева, соперника —
/// отражение через центр доски; ячейка на вид фигуры, пиксели арта.
const RESERVE_AT: Vec2 = vec2(-62.0, 310.0);
const RESERVE_SLOT: Vec2 = vec2(40.0, 58.0);
const RESERVE_GAP: f32 = 3.0;
/// Фигура в ячейке — мельче, чем на доске; основание — на столько ниже верха ячейки.
const RESERVE_SCALE: f32 = 0.6;
const RESERVE_GROUND: f32 = 42.0;

/// С 0.40 карты убраны из интерфейса: колоды, руки и очки не рисуются.
const SHOW_CARDS: bool = false;

/// Rogue-like (с 0.60): золото — монета справа снизу (левый верхний угол, пиксели арта),
/// число — пиксельным шрифтом ×`POINTS_FONT` над ней, как очки у колоды.
const GOLD_AT: Vec2 = vec2(484.0, 330.0);
/// Табличка ходов (rogue-like) — над кнопкой «Конец хода», с зазором, пиксели арта.
const MOVES_GAP: f32 = 3.0;
/// Число ходов, когда они кончились.
const MOVES_SPENT: Color32 = Color32::from_rgb(0xd8, 0x4a, 0x3a);
/// Магазин после уровня — панель посреди доски: место товара (фигуры или герба), промежуток,
/// поля, высота заголовка и кнопок, пиксели арта.
const SHOP_SLOT: Vec2 = vec2(57.0, 72.0);
const SHOP_GAP: f32 = 8.0;
const SHOP_PAD: f32 = 10.0;
const SHOP_TITLE: f32 = 18.0;
const SHOP_BUTTON: f32 = 22.0;
/// Мест в ряду (сундуки, вещи открытого сундука); следующие — рядом ниже.
const SHOP_ROW: usize = 6;
/// Магазин не уже стольких мест (с 0.126 в нём только сундуки).
const SHOP_MIN_COLUMNS: f32 = 3.0;
/// Кнопка «свернуть» — над правым верхним углом магазина, «Магазин» (развернуть) — наверху
/// посреди поля (с 0.126), пиксели арта.
const SHOP_FOLD: Vec2 = vec2(52.0, 14.0);
const SHOP_UNFOLD: Vec2 = vec2(84.0, 20.0);
const SHOP_UNFOLD_TOP: f32 = -31.0;
/// Меню карты в магазине (с 0.126): плашки «Сыграть» и «Продать» под картой, пиксели арта.
const CARD_MENU: Vec2 = vec2(74.0, 18.0);
/// Медали игрока (с 0.126) — в ряд в правом верхнем углу поля: правый верхний край, промежуток.
const MEDALS_AT: Vec2 = vec2(530.0, -32.0);
const MEDAL_GAP: f32 = 4.0;
/// Фигура на прилавке — в долю обычного размера; основание — так низко в ячейке.
const SHOP_PIECE: f32 = 0.75;
const SHOP_GROUND: f32 = 50.0;

/// Кнопка «Конец хода» горит: фигурой сходили, ходить нечем или идёт расстановка.
fn pos_ready(pos: &crate::game::Position, stuck: bool) -> bool {
    pos.moved() || stuck || pos.planning()
}

/// Что под курсором в магазине.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ShopSpot {
    /// Новые сундуки (с 0.126; до того — новый прилавок).
    Reroll,
    Leave,
    /// Сундук под прилавком (с 0.110).
    Chest(usize),
    /// Место в открытом сундуке.
    Pick(usize),
    /// Зона продажи слева от магазина (с 0.112 снова): сюда перетаскивают фигуру из запаса.
    Sell,
    /// «свернуть» — магазин прячется, доска и карты свободны (с 0.126).
    Fold,
}

/// Меню карты в магазине (с 0.126).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CardMenu {
    Play,
    Sell,
}

/// Точка (пиксели арта) на кнопке «Магазин».
pub(super) fn unfold_contains(at: Pos2) -> bool {
    unfold_rect().contains(at)
}

/// Кнопка «Магазин» — развернуть свёрнутый магазин, пиксели арта.
fn unfold_rect() -> Rect {
    Rect::from_min_size(
        Pos2::new(MID.x - SHOP_UNFOLD.x / 2.0, SHOP_UNFOLD_TOP),
        SHOP_UNFOLD,
    )
}

/// Зона продажи — шириной в место прилавка с полями, пиксели арта.
const SELL_WIDTH: f32 = 64.0;
/// Кнопка продажи герба под гербом в колонке, пиксели арта.
const CREST_SELL: Vec2 = vec2(74.0, 18.0);

/// Сундук на своём месте — отступ картинки от верха места, пиксели арта.
const CHEST_TOP: f32 = 7.0;
/// Рука карт сил (rogue-like, с 0.111) — в левом верхнем углу поля: левый верхний угол первой
/// карты и промежуток, пиксели арта; выбранная карта приподнята.
const POWER_HAND_AT: Vec2 = vec2(-62.0, -32.0);
const POWER_HAND_GAP: f32 = 4.0;
const POWER_HAND_LIFT: f32 = 5.0;

/// Гербы (rogue-like, с 0.77): отступ картинки герба от верха места на прилавке, пиксели арта.
const CREST_TOP: f32 = 4.0;
/// Купленные гербы (с 0.114) — колонкой слева от руки и запаса, где была левая панель: левый
/// верхний угол первого герба, промежуток и гербов в ряду, пиксели арта; ряды идут вниз.
const CRESTS_AT: Vec2 = vec2(-174.0, -32.0);
const CREST_GAP: f32 = 4.0;
const CREST_COLUMNS: usize = 2;
/// Подсказка герба: ширина текста, поля, пиксели арта.
const TIP_WIDTH: f32 = 150.0;
const TIP_PAD: f32 = 5.0;

impl BattleChessApp {
    /// Чья рука на месте `seat`: своя сторона — та, что внизу доски.
    pub(super) fn seat_color(&self, seat: Seat) -> Color {
        let near = if self.flipped {
            Color::Black
        } else {
            Color::White
        };
        match seat {
            Seat::Near => near,
            Seat::Far => near.opponent(),
        }
    }

    fn seat_of(&self, color: Color) -> Seat {
        if self.seat_color(Seat::Near) == color {
            Seat::Near
        } else {
            Seat::Far
        }
    }

    fn hand(&self, seat: Seat) -> &[CardKind] {
        self.game.position().cards(self.seat_color(seat)).hand()
    }

    /// Карты руки в веере (номера в руке): без летящих из колоды и без взятой мышью.
    fn hand_layout(&self, seat: Seat) -> Vec<usize> {
        let color = self.seat_color(seat);
        let flying = self.draws.iter().filter(|d| d.color == color).count();
        let held = self
            .held
            .as_ref()
            .filter(|h| h.seat == seat)
            .map(|h| h.index);
        (0..self.hand(seat).len().saturating_sub(flying))
            .filter(|&i| Some(i) != held)
            .collect()
    }

    /// Место карты `index` в веере.
    fn slot_of(&self, seat: Seat, index: usize, lift: f32) -> Slot {
        let layout = self.hand_layout(seat);
        let k = layout
            .iter()
            .position(|&i| i == index)
            .unwrap_or(layout.len());
        seat.slot(k, layout.len().max(k + 1), lift)
    }

    /// Середина крупной карты под курсором: нижним краем она на месте карты, поднятой на
    /// `lift` (у соперника — верхним).
    fn zoomed_center(&self, seat: Seat, index: usize, lift: f32) -> Vec2 {
        let slot = self.slot_of(seat, index, lift);
        slot.center - vec2(0.0, seat.sign() * CARD.y * (ZOOM - 1.0) / 2.0)
    }

    /// Насколько сейчас поднята карта под курсором: она выезжает вверх за `RISE_TIME`.
    fn hover_lift(&self, seat: Seat, now: f64) -> f32 {
        let u = ((now - self.card_hover_since) / RISE_TIME).clamp(0.0, 1.0) as f32;
        self.lift(seat) * ease_out(u)
    }

    /// Карта под курсором ещё выезжает — кадры нужны непрерывно.
    pub(super) fn card_rising(&self, now: f64) -> bool {
        self.card_hover.is_some() && now - self.card_hover_since < RISE_TIME
    }

    /// Картинка карты: свою руку видно всегда, руку соперника — только если за ним
    /// тоже человек.
    fn pic(&self, seat: Seat, kind: CardKind, selected: bool) -> CardPic {
        let color = self.seat_color(seat);
        if seat == Seat::Far && self.computer.is_some() {
            CardPic::Back(color)
        } else {
            CardPic::Front {
                kind,
                color,
                selected,
            }
        }
    }

    /// Может ли человек сейчас сыграть карту из руки на месте `seat`.
    fn can_play_from(&self, seat: Seat) -> bool {
        let pos = self.game.position();
        self.human_to_move()
            && self.promotion.is_none()
            && self.aiming.is_none()
            && pos.side() == self.seat_color(seat)
            && pos.can_play_card()
    }

    fn lift(&self, seat: Seat) -> f32 {
        if self.can_play_from(seat) {
            LIFT
        } else {
            LIFT_IDLE
        }
    }

    /// Карта под курсором — только в самом веере, по узким полосам (`fan_slot_at`).
    /// Крупная карта над веером курсор не держит: увёл курсор с руки — она опускается.
    pub(super) fn card_under(&self, board: &BoardGeom, pointer: Pos2) -> Option<(Seat, usize)> {
        let at = board.to_art(pointer);
        [Seat::Near, Seat::Far]
            .into_iter()
            // Рука соперника-компьютера не отзывается на курсор.
            .filter(|&seat| seat == Seat::Near || self.computer.is_none())
            .find_map(|seat| {
                let layout = self.hand_layout(seat);
                let away = |k: usize| {
                    self.returning_card(seat, layout[k]) || self.aimed_card(seat, layout[k])
                };
                fan_slot_at(seat, layout.len(), at, away).map(|k| (seat, layout[k]))
            })
    }

    /// Сколько карт в колоде на месте `seat` — вместе с теми, что ещё летят в руку: столько
    /// и нарисовано в стопке.
    fn deck_count(&self, seat: Seat) -> u8 {
        let color = self.seat_color(seat);
        let flying = self.draws.iter().filter(|d| d.color == color).count() as u8;
        self.game.position().cards(color).deck_len() + flying
    }

    /// Стопка колоды на месте `seat`: верхняя карта и торцы под ней, как в `deck` из
    /// assets/cards.rs; пустой колоды нет.
    fn deck_rect(&self, seat: Seat) -> Option<Rect> {
        let cards = self.deck_count(seat);
        let edges = cards.min(DECK_EDGES) as f32;
        let size = vec2(DECK_FULL.x, CARD.y + edges + 3.0);
        (cards > 0).then(|| Rect::from_min_size(seat.deck_at().to_pos2(), size))
    }

    /// Колода под курсором.
    pub(super) fn deck_under(&self, board: &BoardGeom, pointer: Pos2) -> Option<Seat> {
        let at = board.to_art(pointer).to_pos2();
        [Seat::Near, Seat::Far]
            .into_iter()
            .find(|&seat| self.deck_rect(seat).is_some_and(|rect| rect.contains(at)))
    }

    /// Курсор над колодой или рукой соперника-компьютера: эти карты не берут.
    pub(super) fn over_closed_cards(&self, board: &BoardGeom, pointer: Pos2) -> bool {
        let at = board.to_art(pointer).to_pos2();
        if self.deck_under(board, pointer).is_some() {
            return true;
        }
        if self.computer.is_none() {
            return false;
        }
        let layout = self.hand_layout(Seat::Far);
        (0..layout.len()).any(|k| covers(Seat::Far.slot(k, layout.len(), 0.0), at.to_vec2()))
    }

    fn returning_card(&self, seat: Seat, index: usize) -> bool {
        self.returning
            .iter()
            .any(|r| r.seat == seat && r.index == index)
    }

    fn aimed_card(&self, seat: Seat, index: usize) -> bool {
        self.aiming
            .as_ref()
            .is_some_and(|a| a.seat == seat && a.index == index)
    }

    /// Курсор над картой, которую можно взять.
    pub(super) fn card_playable(&self) -> bool {
        self.card_hover
            .is_some_and(|(seat, index)| self.usable(seat, index))
    }

    pub(super) fn holding_card(&self) -> bool {
        self.held.is_some()
    }

    pub(super) fn aiming_card(&self) -> bool {
        self.aiming.is_some()
    }

    /// Фигуры, на которые можно направить карту, ждущую цель.
    pub(super) fn aim_targets(&self) -> &[Square] {
        self.aiming.as_ref().map_or(&[], |a| &a.targets)
    }

    /// Первая выбранная фигура обмена.
    pub(super) fn aim_first(&self) -> Option<Square> {
        self.aiming.as_ref().and_then(|a| a.first)
    }

    /// Можно ли щёлкнуть по фигуре на `sq`, пока карта ждёт цель: выбрать её или снять
    /// выбор первой фигуры обмена.
    pub(super) fn aim_pickable(&self, sq: Square) -> bool {
        self.aim_targets().contains(&sq) || self.aim_first() == Some(sq)
    }

    /// Карта на поле, ждущая клетку: как она ляжет вокруг клетки под курсором.
    pub(super) fn aim_area(&self) -> Option<Area> {
        self.aiming.as_ref().and_then(|a| a.kind.area())
    }

    /// Подсказка в строке хода, пока карта ждёт цель.
    pub(super) fn aim_hint(&self) -> &'static str {
        match self.aiming.as_ref().map(|a| (a.kind, a.first.is_some())) {
            Some((CardKind::Swap, false)) => "Обмен: выберите первую свою фигуру (Esc — отмена)",
            Some((CardKind::Heal, _)) => "Исцеление: выберите своего слона (Esc — отмена)",
            Some((CardKind::Swap, true)) => {
                "Обмен: выберите вторую фигуру — они поменяются местами (Esc — отмена)"
            }
            Some((CardKind::Storm, _)) => {
                "Буря: выберите клетку — замёрзнут три горизонтали вокруг неё (Esc — отмена)"
            }
            Some((CardKind::Avalanche, _)) => {
                "Лавина: выберите клетку — замёрзнут три вертикали вокруг неё (Esc — отмена)"
            }
            Some((CardKind::IceBarrier, _)) => {
                "Ледяной барьер: выберите клетку — лёд ляжет квадратом 3×3 вокруг неё (Esc — отмена)"
            }
            Some((CardKind::IceWall, _)) => {
                "Ледяная стена: выберите горизонталь — она обледенеет на два хода (Esc — отмена)"
            }
            Some((CardKind::MassSacrifice, _)) => {
                "Массовое жертвоприношение: выберите квадрат 3×3 со своими фигурами (Esc — отмена)"
            }
            Some((CardKind::Offering, _)) => {
                "Жертва: выберите свою фигуру — она уйдёт с доски (Esc — отмена)"
            }
            Some((CardKind::Multiply, _)) => {
                "Они множатся: выберите свою фигуру, кроме короля и ферзя (Esc — отмена)"
            }
            Some((CardKind::Champion, _)) => {
                "Один в Поле Воин: выберите свою фигуру, кроме короля (Esc — отмена)"
            }
            Some((CardKind::Bastion, _)) => {
                "Оборонительная позиция: выберите свою ладью (Esc — отмена)"
            }
            Some((CardKind::IceShard, _)) => {
                "Ледяной снаряд: выберите фигуру соперника — 500 урона и заморозка (Esc — отмена)"
            }
            Some((CardKind::IceBlast, _)) => {
                "Ледяной взрыв: выберите замёрзшую фигуру соперника (Esc — отмена)"
            }
            Some((CardKind::Barrier, _)) => {
                "Барьер: выберите клетку — соперник не войдёт в квадрат 3×3 вокруг неё (Esc — отмена)"
            }
            Some((CardKind::IceWarrior, _)) => {
                "Воин из Льда: выберите свою фигуру — её удары будут замораживать (Esc — отмена)"
            }
            Some((CardKind::NorthWarrior, _)) => {
                "Северный воин: выберите свою фигуру (Esc — отмена)"
            }
            _ => "Выберите свою фигуру — волна ударит вокруг неё (Esc — отмена)",
        }
    }

    /// Нажатие на карту: карта, которую можно сыграть, отрывается от руки и тянется к курсору.
    pub(super) fn grab_card(&mut self, seat: Seat, index: usize, pointer: Vec2, now: f64) {
        if !self.usable(seat, index) || self.returning_card(seat, index) {
            return;
        }
        let Some(&kind) = self.hand(seat).get(index) else {
            return;
        };
        let start = self.slot_of(seat, index, LIFT).center;
        self.held = Some(Held {
            seat,
            index,
            kind,
            at: start,
            tilt: 0.0,
            pointer,
            last: now,
        });
        self.card_hover = None;
    }

    /// Взятая карта догоняет курсор и покачивается от скорости.
    pub(super) fn carry_card(&mut self, pointer: Option<Vec2>, now: f64) {
        let Some(held) = &mut self.held else {
            return;
        };
        if let Some(pointer) = pointer {
            held.pointer = pointer;
        }
        let dt = (now - held.last).clamp(0.0, 0.1) as f32;
        held.last = now;
        if dt <= 0.0 {
            return;
        }
        let before = held.at;
        held.at += (held.pointer - held.at) * (1.0 - (-dt * FOLLOW).exp());
        let speed = (held.at.x - before.x) / dt;
        let target = (speed * SWAY).clamp(-SWAY_MAX, SWAY_MAX);
        held.tilt += (target - held.tilt) * (1.0 - (-dt * SWAY_EASE).exp());
    }

    /// Кнопку отпустили: над рукой карта возвращается, в другом месте — разыгрывается.
    /// Карта с выбором фигуры или клетки встаёт над рукой и ждёт цель — где бы её ни
    /// отпустили, даже над целью: от короля к курсору тянется стрелка, цель выбирают
    /// щелчком (у карты на поле подсвечены клетки, на которые она ляжет).
    pub(super) fn drop_card(&mut self, pointer: Option<Vec2>, now: f64) {
        let Some(held) = self.held.take() else {
            return;
        };
        let at = pointer.unwrap_or(held.pointer);
        let n = self.hand_layout(held.seat).len() + 1;
        let over_hand = held.seat.hand_zone(n).contains(at.to_pos2());
        if over_hand || !self.can_play_from(held.seat) {
            self.return_card(held, now);
            return;
        }
        if held.kind.targeted() {
            self.aiming = Some(Aiming {
                seat: held.seat,
                index: held.index,
                kind: held.kind,
                first: None,
                targets: self.game.position().card_targets(held.index),
            });
            return;
        }
        self.play_card(held.seat, held.index, &[], held.at, now);
    }

    /// Щелчок с картой, ждущей цель, по своей фигуре: волна бьёт вокруг неё; обмену
    /// первая фигура запоминается, вторая — меняется с ней местами. Повторный щелчок по
    /// первой фигуре снимает выбор.
    pub(super) fn aim_at(&mut self, target: Square, now: f64) {
        let Some(mut aim) = self.aiming.take() else {
            return;
        };
        let pos = self.game.position();
        if aim.first == Some(target) {
            aim.first = None;
            aim.targets = pos.card_targets(aim.index);
        } else if !aim.targets.contains(&target) {
            // Сюда карту не сыграть — ждём дальше.
        } else if aim.kind.picks() == 2 && aim.first.is_none() {
            aim.first = Some(target);
            aim.targets = pos.swap_partners(target);
        } else {
            let targets: Vec<Square> = aim.first.into_iter().chain([target]).collect();
            let center = self.slot_of(aim.seat, aim.index, LIFT).center;
            self.play_card(aim.seat, aim.index, &targets, center, now);
            return;
        }
        self.aiming = Some(aim);
    }

    /// Карта, ждущая цель, возвращается в руку (Esc, правая кнопка, щелчок мимо цели).
    pub(super) fn cancel_aim(&mut self) {
        self.aiming = None;
    }

    /// Правая кнопка или Esc: взятая карта возвращается в руку.
    pub(super) fn cancel_card(&mut self, now: f64) {
        if let Some(held) = self.held.take() {
            self.return_card(held, now);
        }
    }

    /// Карту можно взять: карту с выбором — только если есть свои фигуры для неё.
    fn usable(&self, seat: Seat, index: usize) -> bool {
        self.can_play_from(seat) && self.game.position().card_playable(index)
    }

    /// Карта `index` сгорает там, где она была (`center`); огненная волна расходится
    /// от выбранной фигуры, обмен открывает порталы под двумя фигурами.
    fn play_card(&mut self, seat: Seat, index: usize, targets: &[Square], center: Vec2, now: f64) {
        let color = self.seat_color(seat);
        let next = color.opponent();
        let hand = self.game.position().cards(next).hand().len();
        let Some(play) = self.game.play_card(index, targets) else {
            return;
        };
        self.after_card(play, center, now);
        self.card_hover = None;
        self.aiming = None;
        // Карта после хода фигурой заканчивает ход — соперник берёт карту.
        self.note_draw(next, hand, now + DRAW_DELAY);
    }

    fn after_card(&mut self, play: CardPlay, center: Vec2, now: f64) {
        self.burns.push(Burn {
            kind: play.kind,
            color: play.color,
            center,
            start: now,
        });
        // Взятые картой карты (дополнительное финансирование) летят из колоды одна за другой.
        for k in 0..play.drawn {
            self.draws.push(Draw {
                color: play.color,
                start: now + DRAW_DELAY + f64::from(k) * DRAW_GAP,
            });
        }
        match (play.kind, play.targets.as_slice()) {
            (CardKind::FireWave, &[center]) => self.waves.push(Wave::new(center, play.hits, now)),
            (CardKind::Heal, &[center]) => self.blesses.push(Bless {
                center,
                color: play.color,
                heals: play.heals,
                start: now,
                impacted: false,
            }),
            (CardKind::Swap, &[a, b]) => {
                self.portals.push(Portal {
                    squares: [a, b],
                    start: now,
                    sounded: false,
                    pieces: None,
                });
                // Фигура, вставшая в ледяную зону, замерзает, поднявшись из портала.
                if !play.frozen.is_empty() {
                    let up = now + 2.0 * PORTAL_FRAMES as f64 * PORTAL_FRAME_TIME;
                    for &sq in &play.frozen {
                        self.freezes.push(Freeze::new(sq, &[], &[sq], up));
                    }
                }
            }
            (CardKind::Offering, &[sq]) => {
                // Принесённая в жертву фигура растворяется, как погибшая.
                let before = self
                    .game
                    .history()
                    .iter()
                    .rev()
                    .find_map(|r| match &r.event {
                        Event::Card(p) if p.kind == CardKind::Offering => r.before.piece(sq),
                        _ => None,
                    });
                if let Some(piece) = before {
                    self.effects.push(Effect {
                        square: sq,
                        victim: Some(piece),
                        amount: piece.hp,
                        killed: true,
                        mark: Mark::Hit,
                        start: now,
                        stack: 0,
                    });
                }
            }
            (CardKind::AbsoluteZero, _) => {
                // Лёд нарастает на замёрзших навсегда фигурах, расходясь от первой.
                if let Some(&origin) = play.frozen.first() {
                    self.freezes
                        .push(Freeze::new(origin, &play.frozen, &play.frozen, now));
                }
            }
            (
                CardKind::MassSacrifice
                | CardKind::IceShard
                | CardKind::IceBlast
                | CardKind::HardWinter,
                _,
            ) => {
                // Урон картой — цифры и вспышка на задетых, погибшие растворяются.
                for hit in &play.hits {
                    self.effects.push(Effect {
                        square: hit.square,
                        victim: Some(hit.target),
                        amount: hit.damage,
                        killed: hit.killed,
                        mark: Mark::Hit,
                        start: now,
                        stack: 0,
                    });
                }
                // Выжившую цель ледяного снаряда сковывает лёд.
                for &sq in &play.frozen {
                    self.freezes.push(Freeze::new(sq, &[sq], &[sq], now));
                }
            }
            (kind, &[aim]) if kind.freezes() => {
                // Буря, лавина, ледяной барьер, ледяная стена: лёд расходится от прицела.
                if let Some(area) = kind.area() {
                    let tiles = area.squares(aim);
                    self.freezes
                        .push(Freeze::new(aim, &tiles, &play.frozen, now));
                }
            }
            _ => {}
        }
    }

    fn return_card(&mut self, held: Held, now: f64) {
        self.returning.push(Returning {
            seat: held.seat,
            index: held.index,
            kind: held.kind,
            from: held.at,
            tilt: held.tilt,
            start: now,
        });
    }

    /// После хода сторона, которой ходить, берёт карту — она летит из колоды в руку.
    pub(super) fn note_draw(&mut self, color: Color, hand_before: usize, start: f64) {
        if self.game.position().cards(color).hand().len() > hand_before {
            self.draws.push(Draw { color, start });
        }
    }

    /// Огненная волна доходит до соседей — цифры урона и звук; закончившиеся
    /// анимации карт убираются.
    pub(super) fn tick_cards(&mut self, now: f64) {
        let burn_time = BURN_FRAMES as f64 * BURN_FRAME_TIME;
        self.burns.retain(|b| now - b.start < burn_time);
        self.draws.retain(|d| now - d.start < DRAW_TIME);
        self.returning.retain(|r| now - r.start < RETURN_TIME);
        let mut effects = Vec::new();
        let mut sounds = Vec::new();
        for wave in &mut self.waves {
            if !wave.impacted && now - wave.start >= wave.impact() {
                wave.impacted = true;
                for hit in &wave.hits {
                    effects.push(Effect {
                        square: hit.square,
                        victim: Some(hit.target),
                        amount: hit.damage,
                        killed: hit.killed,
                        mark: Mark::Hit,
                        start: now,
                        stack: 0,
                    });
                }
                if !wave.hits.is_empty() {
                    let killed = wave.hits.iter().any(|h| h.killed);
                    sounds.push(if killed { Sound::Kill } else { Sound::Hit });
                }
            }
        }
        self.waves.retain(|w| now - w.start < w.length());
        // Фигуры обмена показываются из порталов со вспышкой — тогда и звук.
        for portal in &mut self.portals {
            if !portal.sounded && portal.phase(now).is_none_or(|(appear, f)| appear && f >= 4) {
                portal.sounded = true;
                sounds.push(Sound::Move);
            }
        }
        self.portals.retain(|p| p.phase(now).is_some());
        // Свет исцеления доходит до соседних клеток — цифры лечения.
        for bless in &mut self.blesses {
            if !bless.impacted && now - bless.start >= HEAL_IMPACT {
                bless.impacted = true;
                for heal in &bless.heals {
                    effects.push(Effect::note(heal.square, Mark::Heal, heal.amount, now));
                }
                if !bless.heals.is_empty() {
                    sounds.push(Sound::Move);
                }
            }
        }
        let bless_time = BLESS_FRAMES as f64 * BLESS_FRAME_TIME;
        self.blesses.retain(|b| now - b.start < bless_time);
        // Глыба льда встала и звякнула.
        for freeze in &mut self.freezes {
            let clinked = freeze.pieces.iter().any(|&(_, delay)| {
                now >= freeze.start + (delay + FREEZE_CLINK) as f64 * FREEZE_FRAME_TIME
            });
            if !freeze.sounded && clinked {
                freeze.sounded = true;
                sounds.push(Sound::Move);
            }
        }
        self.freezes.retain(|f| now < f.done() + MELT_TIME);
        self.tick_ice(now);
        self.effects.extend(effects);
        for sound in sounds {
            self.sfx.play(sound);
        }
    }

    /// Следит за льдом: оттаявшие фигуры, растаявшие зоны и погибшие замёрзшие фигуры
    /// оставляют таять лёд и снег на своих клетках.
    fn tick_ice(&mut self, now: f64) {
        let (frozen, snowy) = self.ice_masks();
        let (was_frozen, was_snowy) = self.ice_seen;
        let melted = (was_snowy & !snowy) | (was_frozen & !frozen);
        for sq in 0..64u8 {
            if melted & 1 << sq != 0 {
                self.thaws.retain(|&(s, _)| s != sq);
                self.thaws.push((sq, now));
            }
        }
        self.ice_seen = (frozen, snowy);
        self.thaws.retain(|&(_, start)| now - start < MELT_TIME);
    }

    /// Клетки замёрзших фигур и клетки под снегом (замёрзшие фигуры и ледяные зоны).
    pub(super) fn ice_masks(&self) -> (u64, u64) {
        let pos = self.game.position();
        let frozen = pos
            .pieces()
            .filter(|(_, p)| p.is_frozen())
            .fold(0u64, |mask, (sq, _)| mask | 1 << sq);
        (frozen, frozen | pos.ice_zones())
    }

    /// Позиция сменилась без ходов (отмена, новая партия): лёд — как в ней, без таяния.
    pub(super) fn reset_ice(&mut self) {
        self.freezes.clear();
        self.thaws.clear();
        self.ice_seen = self.ice_masks();
    }

    /// Ход фигурой: фигура, вставшая в ледяную зону (и ладья рокировки), замерзает, когда
    /// доедет — в `arrive` секунд.
    pub(super) fn freeze_arrivals(&mut self, squares: &[Square], arrive: f64) {
        for &sq in squares {
            self.freezes.push(Freeze::new(sq, &[], &[sq], arrive));
        }
    }

    /// Снег на клетке `sq`: кадр заморозки и непрозрачность; `None` — снега нет.
    pub(super) fn snow_at(&self, sq: Square, now: f64) -> Option<(usize, f32)> {
        let last = FREEZE_FRAMES - 1;
        let pos = self.game.position();
        let steady = pos.piece(sq).is_some_and(|p| p.is_frozen()) || pos.ice_zones() & 1 << sq != 0;
        for freeze in &self.freezes {
            let Some(frame) = freeze.frame(&freeze.tiles, sq, now) else {
                continue;
            };
            // Лёд ещё не дошёл — клетка пока прежняя.
            let frame = frame?;
            if frame < last || steady {
                return Some((frame.min(last), 1.0));
            }
            // Лёд встал, но держать его некому (буря, лавина) — снег тает.
            let delay = freeze
                .tiles
                .iter()
                .find(|&&(s, _)| s == sq)
                .map_or(0, |t| t.1);
            let melt = ((now - freeze.settled(delay)) / MELT_TIME) as f32;
            return (melt < 1.0).then_some((last, 1.0 - melt.max(0.0)));
        }
        if steady {
            return Some((last, 1.0));
        }
        self.melting(sq, now).map(|alpha| (last, alpha))
    }

    /// Лёд на фигуре `piece` на клетке `sq`: кадр заморозки и непрозрачность.
    pub(super) fn ice_on(&self, sq: Square, piece: Piece, now: f64) -> Option<(usize, f32)> {
        let last = FREEZE_FRAMES - 1;
        for freeze in &self.freezes {
            if let Some(frame) = freeze.frame(&freeze.pieces, sq, now) {
                return frame.map(|f| (f.min(last), 1.0));
            }
        }
        if piece.is_frozen() {
            return Some((last, 1.0));
        }
        self.melting(sq, now).map(|alpha| (last, alpha))
    }

    /// Тает ли лёд на клетке `sq`: сколько его осталось.
    fn melting(&self, sq: Square, now: f64) -> Option<f32> {
        let &(_, start) = self.thaws.iter().find(|&&(s, _)| s == sq)?;
        let u = ((now - start) / MELT_TIME) as f32;
        (u < 1.0).then_some(1.0 - u.max(0.0))
    }

    /// Снег на клетках — до подсветок и фигур, от дальних клеток к ближним (передний
    /// бортик снега ложится на дальние клетки).
    pub(super) fn paint_snow(
        &self,
        painter: &Painter,
        board: &BoardGeom,
        tex: &Textures,
        now: f64,
    ) {
        let mut squares: Vec<(Square, usize, f32)> = (0..64u8)
            .filter_map(|sq| self.snow_at(sq, now).map(|(f, a)| (sq, f, a)))
            .collect();
        squares.sort_by(|a, b| {
            board
                .depth(super::center(a.0))
                .total_cmp(&board.depth(super::center(b.0)))
        });
        for (sq, frame, alpha) in squares {
            if let Some(tile) = tex.ui.freeze_tile(ice_variant(sq), frame) {
                put_frame(painter, board, tile, sq, alpha);
            }
        }
    }

    /// Снежинки, пар и искры заморозки — над фигурами.
    pub(super) fn paint_freeze_over(
        &self,
        painter: &Painter,
        board: &BoardGeom,
        tex: &Textures,
        now: f64,
    ) {
        for freeze in &self.freezes {
            let mut squares: Vec<Square> = freeze.tiles.iter().map(|t| t.0).collect();
            for &(sq, _) in &freeze.pieces {
                if !squares.contains(&sq) {
                    squares.push(sq);
                }
            }
            for sq in squares {
                let list = if freeze.tiles.iter().any(|t| t.0 == sq) {
                    &freeze.tiles
                } else {
                    &freeze.pieces
                };
                if let Some(Some(frame)) = freeze.frame(list, sq, now)
                    && frame < FREEZE_FRAMES
                    && let Some(over) = tex.ui.freeze_over(frame)
                {
                    put_frame(painter, board, over, sq, 1.0);
                }
            }
        }
    }

    pub(super) fn cards_moving(&self) -> bool {
        !self.burns.is_empty()
            || !self.draws.is_empty()
            || !self.returning.is_empty()
            || !self.waves.is_empty()
            || !self.portals.is_empty()
            || !self.blesses.is_empty()
            || !self.freezes.is_empty()
            || !self.thaws.is_empty()
            || self.held.is_some()
    }

    /// Карта ещё действует на доске (огненная волна, обмен, исцеление, заморозка) —
    /// компьютер ждёт.
    pub(super) fn card_effect_running(&self, now: f64) -> bool {
        !self.waves.is_empty()
            || !self.portals.is_empty()
            || !self.blesses.is_empty()
            || self.freezes.iter().any(|f| now < f.done())
    }

    /// Кадр слона с крыльями, если на `sq` сейчас исцеляет слон стороны `color`.
    pub(super) fn bless_frame(&self, sq: Square, color: Color, now: f64) -> Option<usize> {
        self.blesses
            .iter()
            .find(|b| b.center == sq && b.color == color)
            .map(|b| ((now - b.start).max(0.0) / BLESS_FRAME_TIME) as usize)
    }

    /// Кадр исцеления: `over` — слой над фигурами (столбы света, искры), иначе — свет на
    /// земле под слоном и кольцо по соседним клеткам.
    pub(super) fn paint_blesses(
        &self,
        painter: &Painter,
        board: &BoardGeom,
        tex: &Textures,
        over: bool,
        now: f64,
    ) {
        for bless in &self.blesses {
            let frame = ((now - bless.start).max(0.0) / BLESS_FRAME_TIME) as usize;
            let center = board.to_art(board.ground(bless.center));
            let min = board.art_to_screen(center - WAVE_CENTER);
            let rect = Rect::from_min_size(min, WAVE_FRAME * board.scale);
            painter.image(tex.ui.bless(over, frame).id(), rect, UV, Color32::WHITE);
        }
    }

    /// Ход фигурой во время обмена: обмен доигрывается сразу.
    pub(super) fn finish_portals(&mut self) {
        self.portals.clear();
    }

    /// Клетки, фигуры на которых рисует обмен.
    pub(super) fn portal_hides(&self, sq: Square) -> bool {
        self.portals.iter().any(|p| p.squares.contains(&sq))
    }

    /// Фигуры обмена: пока они проваливаются, каждая стоит на своей прежней клетке, потом
    /// поднимается на клетке другой; ушедшая в портал фигура срезана его кромкой.
    pub(super) fn portal_figures(&self, now: f64) -> Vec<Figure> {
        let pos = self.game.position();
        let mut figures = Vec::new();
        for portal in &self.portals {
            let Some((appear, frame)) = portal.phase(now) else {
                continue;
            };
            let sink = Sink {
                depth: PORTAL_SINK[appear as usize][frame],
                scale: PORTAL_SCALE[frame],
            };
            for (k, &sq) in portal.squares.iter().enumerate() {
                // Позиция уже после обмена: прежняя фигура клетки стоит на другой.
                let from = if appear { sq } else { portal.squares[1 - k] };
                let piece = match portal.pieces {
                    Some(pieces) => Some(pieces[if appear { 1 - k } else { k }]),
                    None => pos.piece(from),
                };
                if let Some(piece) = piece {
                    // Пока портал не утянул фигуру, она в своём наряде (аура теневой и т. п.).
                    figures.push(Figure {
                        sink: Some(sink),
                        shadow: sink.depth == 0.0,
                        ..self.dressed(sq, piece, super::center(sq), now)
                    });
                }
            }
        }
        figures
    }

    /// Кадр порталов обмена: `over` — слой над фигурами, иначе — под ними.
    pub(super) fn paint_portals(
        &self,
        painter: &Painter,
        board: &BoardGeom,
        tex: &Textures,
        over: bool,
        now: f64,
    ) {
        for portal in &self.portals {
            let Some((appear, frame)) = portal.phase(now) else {
                continue;
            };
            let texture = tex.ui.portal(appear, over, frame).id();
            for sq in portal.squares {
                let ground = board.to_art(board.ground(sq));
                let min = board.art_to_screen(ground - PORTAL_CENTER);
                let rect = Rect::from_min_size(min, PORTAL_FRAME * board.scale);
                painter.image(texture, rect, UV, Color32::WHITE);
            }
        }
    }

    /// Пока огненная волна не дошла, задетые фигуры стоят как до неё: живые — с прежним
    /// здоровьем, погибшие — ещё на месте.
    pub(super) fn wave_victims(&self) -> Vec<WaveHit> {
        self.waves
            .iter()
            .filter(|w| !w.impacted)
            .flat_map(|w| w.hits.iter().copied())
            .collect()
    }

    /// Кадр огненной волны: `over` — слой над фигурами, иначе — под ними.
    pub(super) fn paint_waves(
        &self,
        painter: &Painter,
        board: &BoardGeom,
        tex: &Textures,
        over: bool,
        now: f64,
    ) {
        for wave in self.waves.iter().filter(|w| now >= w.start) {
            let frame = ((now - wave.start) / WAVE_FRAME_TIME) as usize;
            let center = board.to_art(board.ground(wave.center));
            if wave.wide {
                if let Some(sprite) = tex.ui.big_wave(over, frame) {
                    let min = board.art_to_screen(center - BIG_WAVE_CENTER + sprite.at);
                    let rect = Rect::from_min_size(min, sprite.size * board.scale);
                    painter.image(sprite.texture, rect, UV, Color32::WHITE);
                }
                continue;
            }
            let min = board.art_to_screen(center - WAVE_CENTER);
            let rect = Rect::from_min_size(min, WAVE_FRAME * board.scale);
            painter.image(tex.ui.wave(over, frame).id(), rect, UV, Color32::WHITE);
        }
    }

    /// Над колодой под курсором выезжает табличка с числом карт в ней — как атака и
    /// здоровье у фигур: значок карты в цвет огней стороны и цифры пиксельным шрифтом.
    /// Своя колода внизу — табличка над ней, колода соперника вверху — под ней.
    pub(super) fn paint_deck_count(
        &self,
        painter: &Painter,
        board: &BoardGeom,
        tex: &Textures,
        now: f64,
    ) {
        let Some((seat, since)) = self.deck_hover else {
            return;
        };
        let Some(deck) = self.deck_rect(seat) else {
            return;
        };
        let t = ((now - since) / STATS_TIME).clamp(0.0, 1.0) as f32;
        let out = ease_out_back(t);
        let alpha = (t * 2.0).min(1.0);
        let icon = match self.seat_color(seat) {
            Color::White => DECK_ICON[0],
            Color::Black => DECK_ICON[1],
        };
        let digits = Glyph::digits(self.deck_count(seat).into());
        let text: Vec<(Glyph, Color32)> = std::iter::once((Glyph::Card, icon))
            .chain(digits.into_iter().map(|g| (g, PIXEL_WHITE)))
            .collect();
        // В маленьком окне табличка не мельче, чем нужно для чтения.
        let s = board.scale.max(1.75);
        let size = self.plate_size(&text) * s;
        let slide = DECK_PLATE_SLIDE * (1.0 - out) * s;
        let x = deck.min.x + CARD.x / 2.0;
        let min = match seat {
            Seat::Near => {
                let anchor = board.art_to_screen(vec2(x, deck.min.y - DECK_PLATE_GAP));
                anchor - vec2(size.x / 2.0, size.y - slide)
            }
            Seat::Far => {
                let anchor = board.art_to_screen(vec2(x, deck.max.y + DECK_PLATE_GAP));
                anchor - vec2(size.x / 2.0, slide)
            }
        };
        self.paint_plate(painter, tex, min, s, &text, alpha);
    }

    /// Очки стороны на месте `seat` у её колоды: большие венок победы, череп жертвы и ромб
    /// маны, над ними — число очков пиксельным шрифтом. Свои — справа от колоды внизу слева, очки
    /// соперника — отражение через центр доски.
    fn paint_points(&self, painter: &Painter, board: &BoardGeom, tex: &Textures, seat: Seat) {
        let color = self.seat_color(seat);
        let zoom = EMBLEM_ZOOM as f32;
        let kinds = [Points::Victory, Points::Sacrifice, Points::Mana];
        // Знаки разной высоты стоят вровень низом, числа над ними — на одной линии над
        // самым высоким.
        let tallest = kinds
            .iter()
            .map(|&p| tex.cards.emblem(p, color).size.y * zoom)
            .fold(0.0, f32::max);
        let bottom = DECK_AT.y + DECK_FULL.y;
        let mut x = DECK_AT.x + DECK_FULL.x + POINTS_GAP;
        for points in kinds {
            let sprite = tex.cards.emblem(points, color);
            let size = sprite.size * zoom;
            // Нижним краем знаки — вровень с низом полной колоды.
            let near = vec2(x, bottom - size.y);
            x += size.x + POINTS_SPACING;
            let min = match seat {
                Seat::Near => near,
                Seat::Far => MID * 2.0 - (near + size),
            };
            let center = min + size / 2.0;
            put(painter, board, sprite, center, Vec2::ZERO, zoom);
            let tint = POINTS_TINT[points as usize];
            let tint = Color32::from_rgb((tint >> 16) as u8, (tint >> 8) as u8, tint as u8);
            let n = self.game.position().points(color, points);
            let text: Vec<(Glyph, Color32)> = Glyph::digits(n.into())
                .into_iter()
                .map(|g| (g, tint))
                .collect();
            let px = board.scale * POINTS_FONT;
            let width = self.text_width(&text) * px;
            // Число — над знаком; у соперника стол повёрнут, и число под знаком.
            let height = GLYPH_HEIGHT * POINTS_FONT;
            let top = match seat {
                Seat::Near => bottom - tallest - POINTS_RAISE - height,
                Seat::Far => MID.y * 2.0 - bottom + tallest + POINTS_RAISE,
            };
            let corner = board.art_to_screen(vec2(center.x, top)) - vec2(width / 2.0, 0.0);
            self.pixel_text(painter, tex, corner, px, &text);
        }
    }

    // ----- Запас взятых фигур -----

    /// Ячейки запаса стороны на месте `seat`: вид, сколько готово, сколько ждёт, место
    /// (пиксели арта).
    fn reserve_slots(&self, seat: Seat) -> Vec<(Stock, u8, u8, Rect)> {
        let reserve = self.game.position().reserve(self.seat_color(seat));
        reserve
            .into_iter()
            .enumerate()
            .map(|(i, (stock, ready, waiting))| {
                let min = RESERVE_AT + vec2(i as f32 * (RESERVE_SLOT.x + RESERVE_GAP), 0.0);
                let min = match seat {
                    Seat::Near => min,
                    Seat::Far => MID * 2.0 - min - RESERVE_SLOT,
                };
                (
                    stock,
                    ready,
                    waiting,
                    Rect::from_min_size(min.to_pos2(), RESERVE_SLOT),
                )
            })
            .collect()
    }

    /// Ячейка запаса под курсором.
    pub(super) fn reserve_under(&self, board: &BoardGeom, pointer: Pos2) -> Option<(Seat, Stock)> {
        let at = board.to_art(pointer).to_pos2();
        [Seat::Near, Seat::Far].into_iter().find_map(|seat| {
            self.reserve_slots(seat)
                .into_iter()
                .find(|s| s.3.contains(at))
                .map(|s| (seat, s.0))
        })
    }

    /// Можно ли выставить фигуру `stock` из запаса на месте `seat` прямо сейчас.
    pub(super) fn reserve_usable(&self, seat: Seat, stock: Stock) -> bool {
        let pos = self.game.position();
        self.human_to_move()
            && self.seat_color(seat) == pos.side()
            && self.aiming.is_none()
            && pos
                .legal_moves()
                .iter()
                .any(|m| m.kind == stock.drop_kind())
    }

    /// Фигура запаса или прилавка в прямоугольнике спрайта `rect`: огненная — в огне
    /// (кадры ожидания лавы), электрическая — в молниях; всё — внутри ячейки `clip`.
    #[allow(clippy::too_many_arguments)]
    fn paint_stock(
        &self,
        painter: &Painter,
        tex: &Textures,
        color: Color,
        stock: Stock,
        rect: Rect,
        clip: Rect,
        tint: Color32,
    ) {
        let painter = &painter.with_clip_rect(clip.intersect(painter.clip_rect()));
        let px = rect.width() / SPRITE.x;
        let piece = stock.piece(color);
        // Объединённая — по второй силе (у варвара своих кадров нет).
        let animated = if stock.with != Power::Plain {
            stock.with
        } else {
            stock.power
        };
        match animated {
            Power::Plain | Power::Gold | Power::Ice | Power::Barbarian | Power::Wizard => {}
            Power::Fire => {
                let [from, to] = LAVA_HOLD;
                let frame = from + (self.now / LAVA_FRAME_TIME) as usize % (to - from);
                if let Some(sprite) = tex.ui.lava(piece, frame) {
                    let at = Rect::from_min_size(rect.min + sprite.at * px, sprite.size * px);
                    painter.image(sprite.texture, at, UV, tint);
                    return;
                }
            }
            Power::Spark | Power::Wind | Power::Shade => {
                let [from, to] = match animated {
                    Power::Wind => WIND_LOOP,
                    Power::Shade => DARK_LOOP,
                    _ => ELECTRIC_LOOP,
                };
                let frame = from + (self.now / ELECTRIC_FRAME_TIME) as usize % (to - from);
                let sprite = match animated {
                    Power::Wind => tex.ui.wind(piece, frame),
                    Power::Shade => tex.ui.dark(0, piece, frame),
                    _ => tex.ui.electric(piece, frame),
                };
                if let Some(sprite) = sprite {
                    // Кадр электричества больше холста фигуры: совмещаются точки опоры.
                    let origin = rect.min + (ANCHOR - ELECTRIC_ANCHOR) * px;
                    let at = Rect::from_min_size(origin + sprite.at * px, sprite.size * px);
                    painter.image(sprite.texture, at, UV, tint);
                    return;
                }
            }
        }
        painter.image(tex.piece_body(piece), rect, UV, tint);
    }

    /// Запас обеих сторон: фигура, под ней — сколько готово (×N) и сколько ждёт (+N).
    fn paint_reserve(&self, painter: &Painter, board: &BoardGeom, tex: &Textures) {
        let s = board.scale;
        for seat in [Seat::Near, Seat::Far] {
            let color = self.seat_color(seat);
            for (stock, ready, waiting, slot) in self.reserve_slots(seat) {
                let rect = Rect::from_min_max(
                    board.art_to_screen(slot.min.to_vec2()),
                    board.art_to_screen(slot.max.to_vec2()),
                );
                let usable = self.reserve_usable(seat, stock) || self.reserve_sellable(seat, stock);
                let picked = usable && self.drop_pick == Some(stock);
                let hovered = usable && self.reserve_hover == Some((seat, stock));
                let shadow = rect.translate(vec2(2.0, 3.0) * s);
                painter.rect_filled(shadow, CornerRadius::ZERO, Color32::from_black_alpha(77));
                painter.rect_filled(
                    rect,
                    CornerRadius::ZERO,
                    Color32::from_rgb(0x0d, 0x0f, 0x17),
                );
                let inner = rect.shrink(s);
                let fill = if picked {
                    Color32::from_rgb(0x2a, 0x45, 0x5d)
                } else if hovered {
                    Color32::from_rgb(0x2a, 0x35, 0x4d)
                } else {
                    Color32::from_rgb(0x18, 0x1c, 0x29)
                };
                painter.rect_filled(inner, CornerRadius::ZERO, fill);
                let edge = if picked || hovered {
                    Color32::from_rgb(0x9f, 0xb3, 0xd1)
                } else if usable {
                    Color32::from_rgb(0x5f, 0xc8, 0xe0)
                } else {
                    Color32::from_rgb(0x2a, 0x33, 0x45)
                };
                painter.rect_stroke(
                    inner,
                    CornerRadius::ZERO,
                    Stroke::new(s, edge),
                    StrokeKind::Inside,
                );
                let ground = rect.min + vec2(RESERVE_SLOT.x / 2.0, RESERVE_GROUND) * s;
                let sprite = Rect::from_min_size(
                    board.snap(ground - ANCHOR * s * RESERVE_SCALE),
                    SPRITE * s * RESERVE_SCALE,
                );
                let tint = if ready > 0 {
                    Color32::WHITE
                } else {
                    Color32::from_white_alpha(110)
                };
                self.paint_stock(painter, tex, color, stock, sprite, inner, tint);
                let mut label = String::new();
                if ready > 0 {
                    label += &format!("×{ready}");
                }
                if waiting > 0 {
                    if !label.is_empty() {
                        label.push(' ');
                    }
                    label += &format!("+{waiting}");
                }
                let text = if ready > 0 {
                    theme::TEXT_BRIGHT
                } else {
                    Color32::from_rgb(0x6c, 0x76, 0x8a)
                };
                painter.text(
                    rect.min + vec2(RESERVE_SLOT.x / 2.0, RESERVE_SLOT.y - 7.0) * s,
                    Align2::CENTER_CENTER,
                    label,
                    FontId::new(8.0 * s, theme::bold()),
                    text,
                );
            }
        }
    }

    // ----- Rogue-like: золото и магазин -----

    /// Панель магазина посреди доски и места на ней (пиксели арта): с 0.126 только сундуки
    /// рядами по `SHOP_ROW`, ниже — «Обновить» (новые сундуки) и «Уйти», над правым верхним
    /// углом — «свернуть». Открыт сундук — вместо этого панель сундука с вещами на выбор.
    fn shop_layout(&self) -> (Rect, Vec<(ShopSpot, Rect)>) {
        let pos = self.game.position();
        let open = pos.chest_open();
        let n = match open {
            Some(chest) => pos.chest_wares(chest).len(),
            None => pos.chests().len(),
        }
        .max(1);
        let columns = n.min(SHOP_ROW) as f32;
        let rows = n.div_ceil(SHOP_ROW) as f32;
        let row_width = |k: f32| k * SHOP_SLOT.x + (k - 1.0) * SHOP_GAP;
        let width = row_width(columns.max(SHOP_MIN_COLUMNS)) + 2.0 * SHOP_PAD;
        let height = if open.is_some() {
            SHOP_PAD + SHOP_TITLE + rows * (SHOP_SLOT.y + SHOP_GAP) - SHOP_GAP + SHOP_PAD
        } else {
            SHOP_PAD + SHOP_TITLE + rows * (SHOP_SLOT.y + SHOP_GAP) + SHOP_BUTTON + SHOP_PAD
        };
        let panel = Rect::from_min_size(
            Pos2::new(MID.x - width / 2.0, MID.y - height / 2.0),
            vec2(width, height),
        );
        let mut spots = Vec::new();
        let top = panel.min.y + SHOP_PAD + SHOP_TITLE;
        let first = panel.center().x - row_width(columns) / 2.0;
        for i in 0..n {
            let (row, col) = ((i / SHOP_ROW) as f32, (i % SHOP_ROW) as f32);
            let at = Pos2::new(
                first + col * (SHOP_SLOT.x + SHOP_GAP),
                top + row * (SHOP_SLOT.y + SHOP_GAP),
            );
            let spot = if open.is_some() {
                ShopSpot::Pick(i)
            } else {
                ShopSpot::Chest(i)
            };
            spots.push((spot, Rect::from_min_size(at, SHOP_SLOT)));
        }
        spots.push((
            ShopSpot::Fold,
            Rect::from_min_size(
                Pos2::new(panel.max.x - SHOP_FOLD.x, panel.min.y - SHOP_FOLD.y - 3.0),
                SHOP_FOLD,
            ),
        ));
        if open.is_some() {
            return (panel, spots);
        }
        spots.push((
            ShopSpot::Sell,
            Rect::from_min_size(
                Pos2::new(panel.min.x - SHOP_GAP - SELL_WIDTH, panel.min.y),
                vec2(SELL_WIDTH, height),
            ),
        ));
        let y = top + rows * (SHOP_SLOT.y + SHOP_GAP);
        let half = (width - 2.0 * SHOP_PAD - SHOP_GAP) / 2.0;
        let left = panel.min.x + SHOP_PAD;
        spots.push((
            ShopSpot::Reroll,
            Rect::from_min_size(Pos2::new(left, y), vec2(half, SHOP_BUTTON)),
        ));
        spots.push((
            ShopSpot::Leave,
            Rect::from_min_size(
                Pos2::new(left + half + SHOP_GAP, y),
                vec2(half, SHOP_BUTTON),
            ),
        ));
        (panel, spots)
    }

    /// Место магазина под курсором.
    pub(super) fn shop_under(&self, board: &BoardGeom, pointer: Pos2) -> Option<ShopSpot> {
        let at = board.to_art(pointer).to_pos2();
        self.shop_layout()
            .1
            .into_iter()
            .find(|(_, rect)| rect.contains(at))
            .map(|(spot, _)| spot)
    }

    /// Можно ли сейчас нажать это место магазина: на покупку и обновление хватает золота.
    pub(super) fn shop_usable(&self, spot: ShopSpot) -> bool {
        let pos = self.game.position();
        let gold = pos.rogue().map_or(0, |r| r.gold);
        match spot {
            ShopSpot::Reroll => pos.reroll_price() <= gold && pos.chest_open().is_none(),
            ShopSpot::Leave | ShopSpot::Fold => true,
            ShopSpot::Chest(slot) => {
                pos.chest_for_sale(slot) && pos.chest(slot).is_some_and(|o| o.price() <= gold)
            }
            ShopSpot::Pick(i) => pos
                .chest_open()
                .is_some_and(|chest| pos.chest_wares(chest).get(i).copied().flatten().is_some()),
            ShopSpot::Sell => self.sell_drag.is_some(),
        }
    }

    /// Купленные гербы колонкой слева (пиксели экрана), по порядку покупки — по
    /// `CREST_COLUMNS` в ряд сверху вниз. Не помещаются до низа поля — ряды сходятся и
    /// наползают друг на друга (следующий рисуется поверх).
    fn crest_column(&self, board: &BoardGeom) -> Vec<(Crest, Rect)> {
        let crests = self.game.position().crests();
        let rows = crests.len().div_ceil(CREST_COLUMNS);
        let room = VIEW_MAX.y - CRESTS_AT.y - CREST.y;
        let step = (CREST.y + CREST_GAP).min(room / rows.saturating_sub(1).max(1) as f32);
        crests
            .into_iter()
            .enumerate()
            .map(|(i, crest)| {
                let (row, col) = ((i / CREST_COLUMNS) as f32, (i % CREST_COLUMNS) as f32);
                let min = CRESTS_AT + vec2(col * (CREST.x + CREST_GAP), row * step);
                let at = board.snap(board.art_to_screen(min));
                (crest, Rect::from_min_size(at, CREST * board.scale))
            })
            .collect()
    }

    /// Герб или карта силы под курсором и место на экране — для подсказки: купленный герб в
    /// колонке, карта в руке, герб или карта на прилавке и в сундуке.
    pub(super) fn crest_under(&self, board: &BoardGeom, pointer: Pos2) -> Option<(Ware, Rect)> {
        if let Some((crest, rect)) = self
            .crest_column(board)
            .into_iter()
            .rev()
            .find(|(_, rect)| rect.contains(pointer))
        {
            return Some((Ware::Crest(crest), rect));
        }
        let at = board.to_art(pointer).to_pos2();
        if let Some((_, card, rect)) = self
            .hand_slots()
            .into_iter()
            .find(|(.., rect)| rect.contains(at))
        {
            return Some((
                Ware::Card(card),
                Rect::from_min_max(
                    board.art_to_screen(rect.min.to_vec2()),
                    board.art_to_screen(rect.max.to_vec2()),
                ),
            ));
        }
        if let Some((kind, _, rect)) = self
            .medal_row()
            .into_iter()
            .find(|(.., rect)| rect.contains(at))
        {
            return Some((
                Ware::Medal(kind),
                Rect::from_min_max(
                    board.art_to_screen(rect.min.to_vec2()),
                    board.art_to_screen(rect.max.to_vec2()),
                ),
            ));
        }
        if !self.shop_shown() {
            return None;
        }
        let pos = self.game.position();
        let chest = pos
            .chest_open()
            .map(|c| pos.chest_wares(c))
            .unwrap_or_default();
        self.shop_layout().1.into_iter().find_map(|(spot, rect)| {
            let ware = match spot {
                ShopSpot::Pick(i) => chest.get(i),
                _ => None,
            };
            match ware.copied().flatten().filter(|_| rect.contains(at))? {
                ware @ (Ware::Crest(_) | Ware::Card(_) | Ware::Medal(_)) => Some((
                    ware,
                    Rect::from_min_max(
                        board.art_to_screen(rect.min.to_vec2()),
                        board.art_to_screen(rect.max.to_vec2()),
                    ),
                )),
                Ware::Piece(_) => None,
            }
        })
    }

    /// Купленные гербы — колонкой слева; выбранный на продажу — в золотой рамке, под ним —
    /// кнопка «Продать» с ценой.
    fn paint_crests(&self, painter: &Painter, board: &BoardGeom, tex: &Textures) {
        let s = board.scale;
        for (index, (crest, rect)) in self.crest_column(board).into_iter().enumerate() {
            painter.image(tex.ui.crest(crest).id(), rect, UV, Color32::WHITE);
            self.paint_crest_badge(painter, tex, rect, crest);
            if self.crest_sell == Some(index) && self.crest_sell_spot(board).is_some() {
                painter.rect_stroke(
                    rect.expand(s),
                    CornerRadius::ZERO,
                    Stroke::new(s, Color32::from_rgb(0xff, 0xe2, 0x7a)),
                    StrokeKind::Inside,
                );
            }
        }
        if let Some((_, rect)) = self.crest_sell_spot(board) {
            let edge = if self.crest_sell_hover {
                Color32::from_rgb(0xff, 0xe2, 0x7a)
            } else {
                Color32::from_rgb(0xd0, 0x7d, 0x1c)
            };
            self.shop_plate(painter, board, rect, self.crest_sell_hover, edge);
            let center = rect.center().to_vec2();
            painter.text(
                board.art_to_screen(vec2(rect.min.x + 23.0, center.y)),
                Align2::CENTER_CENTER,
                "Продать",
                FontId::new(7.0 * s, theme::bold()),
                theme::TEXT_BRIGHT,
            );
            self.paint_price(
                painter,
                board,
                tex,
                vec2(rect.max.x - 16.0, center.y),
                crest_sell_price(),
                theme::GOLD,
            );
        }
    }

    /// Место в списке гербов (`Position::crests`) герба колонки под курсором (экранные точки);
    /// наползающие — верхний.
    pub(super) fn column_crest_under(&self, board: &BoardGeom, pointer: Pos2) -> Option<usize> {
        self.crest_column(board)
            .into_iter()
            .enumerate()
            .rev()
            .find(|(_, (_, rect))| rect.contains(pointer))
            .map(|(index, _)| index)
    }

    /// Кнопка продажи выбранного герба (пиксели арта) — под ним, пока магазин ждёт.
    pub(super) fn crest_sell_spot(&self, board: &BoardGeom) -> Option<(usize, Rect)> {
        let index = self.crest_sell?;
        if !self.shop_shown() || !self.game.position().can_sell() {
            return None;
        }
        let (_, rect) = *self.crest_column(board).get(index)?;
        // Под гербом от его левого края, но не правее колонки (не заходит на руку карт).
        let right = CRESTS_AT.x + CREST_COLUMNS as f32 * (CREST.x + CREST_GAP) - CREST_GAP;
        let (left, bottom) = (board.to_art(rect.min).x, board.to_art(rect.max).y);
        let min = Pos2::new(left.min(right - CREST_SELL.x), bottom + 2.0);
        Some((index, Rect::from_min_size(min, CREST_SELL)))
    }

    /// Rogue-like: фигуру `stock` из запаса на месте `seat` можно продать — магазин ждёт, запас
    /// свой и она готова.
    pub(super) fn reserve_sellable(&self, seat: Seat, stock: Stock) -> bool {
        let pos = self.game.position();
        self.shop_shown()
            && pos.can_sell()
            && self.human_to_move()
            && pos
                .rogue()
                .is_some_and(|r| r.player == self.seat_color(seat))
            && pos
                .reserve(self.seat_color(seat))
                .iter()
                .any(|&(s, ready, _)| s == stock && ready > 0)
    }

    /// Подсказка герба или карты силы под курсором: название и что даёт — на плашке под его
    /// местом (не помещается — над ним), в пределах поля.
    fn paint_crest_tip(&self, painter: &Painter, board: &BoardGeom) {
        let Some((ware, anchor)) = self.crest_hover else {
            return;
        };
        let (name, description) = match ware {
            Ware::Crest(crest) => {
                // Гербы с числами складываются (с 0.124): сколько таких уже есть.
                let mut text = crest.description();
                if crest.stacks() {
                    let count = self.game.position().crest_count(crest);
                    text += &match count {
                        0 => " Складывается: можно купить ещё.".to_owned(),
                        n => format!(" Складывается: можно купить ещё (у вас — {n})."),
                    };
                }
                (crest.title().to_owned(), text)
            }
            Ware::Card(card) => {
                let mut text = card.description();
                // Маска: во что превратится сейчас.
                if card == Card::Mask {
                    text += &match self.game.position().last_card() {
                        Some(last) => format!(" Сейчас — {}.", last.title().to_lowercase()),
                        None => " Карт ещё не играли.".to_owned(),
                    };
                }
                (card.title().to_owned(), text)
            }
            Ware::Medal(kind) => {
                let mut text = medal_description(kind);
                let owned = self.game.position().medals()[kind as usize];
                if owned > 0 {
                    text += &format!(" У вас — {owned}.");
                }
                (medal_title(kind).to_owned(), text)
            }
            Ware::Piece(stock) => (stock.name(), String::new()),
        };
        let s = board.scale;
        let pad = TIP_PAD * s;
        let wrap = TIP_WIDTH * s;
        let title = painter.layout(name, FontId::new(8.0 * s, theme::bold()), theme::GOLD, wrap);
        let text = painter.layout(
            description,
            FontId::new(7.0 * s, theme::bold()),
            theme::TEXT_BRIGHT,
            wrap,
        );
        let inner = vec2(
            title.size().x.max(text.size().x),
            title.size().y + 2.0 * s + text.size().y,
        );
        let size = inner + Vec2::splat(2.0 * pad);
        let area = board.area;
        let below = anchor.max.y + 3.0 * s;
        let (x, y) = if self.crest_column(board).iter().any(|&(_, r)| r == anchor) {
            // Герб колонки — подсказка справа от него, не закрывая соседние гербы.
            let y = anchor.min.y.min(area.max.y - size.y).max(area.min.y);
            (anchor.max.x + 3.0 * s, y)
        } else if below + size.y <= area.max.y {
            (anchor.center().x - size.x / 2.0, below)
        } else {
            (
                anchor.center().x - size.x / 2.0,
                anchor.min.y - 3.0 * s - size.y,
            )
        };
        let x = x.min(area.max.x - size.x).max(area.min.x);
        let rect = Rect::from_min_size(board.snap(Pos2::new(x, y)), size);
        painter.rect_filled(
            rect.translate(vec2(2.0, 3.0) * s),
            CornerRadius::ZERO,
            Color32::from_black_alpha(77),
        );
        painter.rect_filled(
            rect,
            CornerRadius::ZERO,
            Color32::from_rgb(0x0d, 0x0f, 0x17),
        );
        let inner_rect = rect.shrink(s);
        painter.rect_filled(
            inner_rect,
            CornerRadius::ZERO,
            Color32::from_rgb(0x18, 0x1c, 0x29),
        );
        painter.rect_stroke(
            inner_rect,
            CornerRadius::ZERO,
            Stroke::new(s, Color32::from_rgb(0x6f, 0xb7, 0xd6)),
            StrokeKind::Inside,
        );
        let corner = rect.min + Vec2::splat(pad);
        let title_height = title.size().y;
        painter.galley(corner, title, theme::GOLD);
        painter.galley(
            corner + vec2(0.0, title_height + 2.0 * s),
            text,
            theme::TEXT_BRIGHT,
        );
    }

    /// Плашка в стиле ячеек запаса: тень, тёмный контур, заливка и кромка.
    fn shop_plate(
        &self,
        painter: &Painter,
        board: &BoardGeom,
        rect: Rect,
        lit: bool,
        edge: Color32,
    ) {
        let s = board.scale;
        let rect = Rect::from_min_max(
            board.art_to_screen(rect.min.to_vec2()),
            board.art_to_screen(rect.max.to_vec2()),
        );
        let shadow = rect.translate(vec2(2.0, 3.0) * s);
        painter.rect_filled(shadow, CornerRadius::ZERO, Color32::from_black_alpha(77));
        painter.rect_filled(
            rect,
            CornerRadius::ZERO,
            Color32::from_rgb(0x0d, 0x0f, 0x17),
        );
        let inner = rect.shrink(s);
        let fill = if lit {
            Color32::from_rgb(0x2a, 0x35, 0x4d)
        } else {
            Color32::from_rgb(0x18, 0x1c, 0x29)
        };
        painter.rect_filled(inner, CornerRadius::ZERO, fill);
        painter.rect_stroke(
            inner,
            CornerRadius::ZERO,
            Stroke::new(s, edge),
            StrokeKind::Inside,
        );
    }

    /// Цена: маленькая монета и число пиксельным шрифтом, по центру точки `center` (арт).
    fn paint_price(
        &self,
        painter: &Painter,
        board: &BoardGeom,
        tex: &Textures,
        center: Vec2,
        price: u16,
        tint: Color32,
    ) {
        let text: Vec<(Glyph, Color32)> = Glyph::digits(price.into())
            .into_iter()
            .map(|g| (g, tint))
            .collect();
        let width = self.text_width(&text);
        let gap = 3.0;
        let left = center.x - (COIN_SMALL.x + gap + width) / 2.0;
        let coin = Rect::from_min_size(
            board.snap(board.art_to_screen(vec2(left, center.y - COIN_SMALL.y / 2.0))),
            COIN_SMALL * board.scale,
        );
        painter.image(tex.ui.coin(false).id(), coin, UV, Color32::WHITE);
        let corner = board.art_to_screen(vec2(
            left + COIN_SMALL.x + gap,
            center.y - GLYPH_HEIGHT / 2.0,
        ));
        self.pixel_text(painter, tex, corner, board.scale, &text);
    }

    /// Rogue-like: сколько ходов фигурами осталось в этом ходу — табличка над «Конец хода»
    /// (пешка и число; кончились — число красное). Воздушная фигура тратит ход первым шагом,
    /// следующие её шаги даровые и счётчик не убавляют.
    fn paint_moves_left(&self, painter: &Painter, board: &BoardGeom, tex: &Textures) {
        let pos = self.game.position();
        let playing = pos.rogue().is_some_and(|r| r.player == pos.side())
            && !pos.planning()
            && !pos.shop_open()
            && self.game.status() == Status::Ongoing;
        if !playing {
            return;
        }
        let left = pos.moves_left();
        let color = if left > 0 { PIXEL_WHITE } else { MOVES_SPENT };
        let text: Vec<(Glyph, Color32)> = [(Glyph::Pawn, PIXEL_WHITE)]
            .into_iter()
            .chain(Glyph::digits(left.into()).into_iter().map(|g| (g, color)))
            .collect();
        let s = board.scale;
        let size = self.plate_size(&text);
        let top = board.art_to_screen(vec2(BUTTON_AT.x + BUTTON.x / 2.0, BUTTON_AT.y - MOVES_GAP));
        let min = board.snap(top - vec2(size.x / 2.0, size.y) * s);
        self.paint_plate(painter, tex, min, s, &text, 1.0);
    }

    /// Золото игрока справа снизу: большая монета, число над ней.
    /// Число герба мульти (с 0.119) — «+2», «×1.5» из баланса — пиксельным шрифтом под знаком;
    /// `rect` — картинка герба на экране.
    fn paint_crest_badge(&self, painter: &Painter, tex: &Textures, rect: Rect, crest: Crest) {
        let Some(badge) = self.game.position().crest_badge(crest) else {
            return;
        };
        let text = paint(&Glyph::text(&badge), MULT_RED);
        let px = rect.width() / CREST.x;
        let width = self.text_width(&text) * px;
        let corner = Pos2::new(rect.center().x - width / 2.0, rect.min.y + BADGE_TOP * px);
        self.pixel_text(painter, tex, corner, px, &text);
    }

    /// Мульти игрока (с 0.119) — над золотом: «×1», «×2.25».
    fn paint_mult(&self, painter: &Painter, board: &BoardGeom, tex: &Textures) {
        let pos = self.game.position();
        if pos.rogue().is_none() {
            return;
        }
        let text = paint(
            &Glyph::text(&format!("×{}", mult_text(pos.mult()))),
            MULT_RED,
        );
        let px = board.scale * POINTS_FONT;
        let width = self.text_width(&text) * px;
        let top = GOLD_AT.y - POINTS_RAISE - 2.0 * GLYPH_HEIGHT * POINTS_FONT - MULT_GAP;
        let mut corner =
            board.art_to_screen(vec2(GOLD_AT.x + COIN.x / 2.0, top)) - vec2(width / 2.0, 0.0);
        // Длинное число — не правее поля (справа — панель).
        corner.x = corner.x.min(board.art_to_screen(VIEW_MAX).x - width);
        self.pixel_text(painter, tex, corner, px, &text);
    }

    pub(super) fn paint_gold(&self, painter: &Painter, board: &BoardGeom, tex: &Textures) {
        let Some(r) = self.game.position().rogue() else {
            return;
        };
        let coin =
            Rect::from_min_size(board.snap(board.art_to_screen(GOLD_AT)), COIN * board.scale);
        painter.image(tex.ui.coin(true).id(), coin, UV, Color32::WHITE);
        let text: Vec<(Glyph, Color32)> = Glyph::digits(r.gold.into())
            .into_iter()
            .map(|g| (g, theme::GOLD))
            .collect();
        let px = board.scale * POINTS_FONT;
        let width = self.text_width(&text) * px;
        let top = GOLD_AT.y - POINTS_RAISE - GLYPH_HEIGHT * POINTS_FONT;
        let corner =
            board.art_to_screen(vec2(GOLD_AT.x + COIN.x / 2.0, top)) - vec2(width / 2.0, 0.0);
        self.pixel_text(painter, tex, corner, px, &text);
    }

    /// Магазин после уровня: фигуры и гербы на прилавке с ценами, под ними сундуки, «Обновить»
    /// (с ценой — растёт с каждым обновлением) и «Уйти»; открыт сундук — его вещи на выбор.
    fn paint_shop(&self, painter: &Painter, board: &BoardGeom, tex: &Textures) {
        let pos = self.game.position();
        if !self.shop_ready() {
            return;
        }
        // Свёрнут (с 0.126) — только кнопка «Магазин» наверху посреди поля.
        if !self.shop_shown() {
            let rect = unfold_rect();
            let edge = if self.unfold_hover {
                Color32::from_rgb(0xff, 0xe2, 0x7a)
            } else {
                Color32::from_rgb(0xd0, 0x7d, 0x1c)
            };
            self.shop_plate(painter, board, rect, self.unfold_hover, edge);
            painter.text(
                board.art_to_screen(rect.center().to_vec2()),
                Align2::CENTER_CENTER,
                "Магазин",
                FontId::new(9.0 * board.scale, theme::bold()),
                theme::GOLD,
            );
            return;
        }
        let s = board.scale;
        let (panel, spots) = self.shop_layout();
        let open = pos.chest_open();
        self.shop_plate(
            painter,
            board,
            panel,
            false,
            Color32::from_rgb(0x8a, 0x45, 0x10),
        );
        let title = match open.and_then(|slot| pos.chest(slot)) {
            Some(offer) => {
                let left = offer.picks() - usize::from(offer.taken);
                let what = match left {
                    1 => "одно".to_owned(),
                    n => n.to_string(),
                };
                format!("{} · выберите {what}", offer.title().to_uppercase())
            }
            None => "МАГАЗИН".to_owned(),
        };
        let title_at = board.art_to_screen(vec2(panel.center().x, panel.min.y + SHOP_PAD + 6.0));
        painter.text(
            title_at,
            Align2::CENTER_CENTER,
            title,
            FontId::new(10.0 * s, theme::bold()),
            theme::GOLD,
        );
        let chest = open.map(|c| pos.chest_wares(c)).unwrap_or_default();
        let dim = Color32::from_rgb(0x6c, 0x76, 0x8a);
        for (spot, rect) in spots {
            let usable = self.shop_usable(spot);
            let hovered = usable && self.shop_hover == Some(spot);
            let edge = if hovered {
                Color32::from_rgb(0xff, 0xe2, 0x7a)
            } else if usable {
                Color32::from_rgb(0xd0, 0x7d, 0x1c)
            } else {
                Color32::from_rgb(0x2a, 0x33, 0x45)
            };
            self.shop_plate(painter, board, rect, hovered, edge);
            let center = rect.center().to_vec2();
            let tint = if usable {
                Color32::WHITE
            } else {
                Color32::from_white_alpha(110)
            };
            let price_tint = if usable { theme::GOLD } else { theme::RED };
            let note = |text: &str| {
                painter.text(
                    board.art_to_screen(center),
                    Align2::CENTER_CENTER,
                    text,
                    FontId::new(7.0 * s, theme::bold()),
                    dim,
                );
            };
            match spot {
                ShopSpot::Fold => {
                    painter.text(
                        board.art_to_screen(center),
                        Align2::CENTER_CENTER,
                        "свернуть",
                        FontId::new(7.0 * s, theme::bold()),
                        theme::TEXT_BRIGHT,
                    );
                }
                ShopSpot::Pick(i) => {
                    // Цены нет — вещь ниже, посередине места; карта — во всё место.
                    if let Some(ware) = chest.get(i).copied().flatten() {
                        let shift = if matches!(ware, Ware::Card(_)) {
                            1.0
                        } else {
                            8.0
                        };
                        let lower = rect.translate(vec2(0.0, shift));
                        self.paint_ware(painter, board, tex, lower, ware, tint);
                    }
                }
                ShopSpot::Chest(slot) => {
                    let Some(offer) = pos.chest(slot) else {
                        continue;
                    };
                    if offer.state == ChestState::Taken {
                        note("открыт");
                        continue;
                    }
                    // Большой — крупнее и выше: дно на той же линии.
                    let size = if offer.big { BIG_CHEST } else { CHEST };
                    let top = rect.min.y + CHEST_TOP + CHEST.y - size.y;
                    let picture = Rect::from_min_size(
                        board.snap(
                            board.art_to_screen(vec2(center.x - (size.x / 2.0).floor(), top)),
                        ),
                        size * s,
                    );
                    painter.image(tex.ui.chest(offer.kind, offer.big).id(), picture, UV, tint);
                    self.paint_price(
                        painter,
                        board,
                        tex,
                        vec2(center.x, rect.max.y - 11.0),
                        offer.price(),
                        price_tint,
                    );
                }
                ShopSpot::Reroll => {
                    let label_at = board.art_to_screen(vec2(rect.min.x + 26.0, center.y));
                    painter.text(
                        label_at,
                        Align2::CENTER_CENTER,
                        "Обновить",
                        FontId::new(8.0 * s, theme::bold()),
                        if usable { theme::TEXT_BRIGHT } else { dim },
                    );
                    self.paint_price(
                        painter,
                        board,
                        tex,
                        vec2(rect.max.x - 17.0, center.y),
                        pos.reroll_price(),
                        price_tint,
                    );
                }
                ShopSpot::Leave => {
                    painter.text(
                        board.art_to_screen(center),
                        Align2::CENTER_CENTER,
                        "Уйти",
                        FontId::new(8.0 * s, theme::bold()),
                        theme::TEXT_BRIGHT,
                    );
                }
                ShopSpot::Sell => {
                    painter.text(
                        board.art_to_screen(vec2(center.x, rect.min.y + SHOP_PAD + 6.0)),
                        Align2::CENTER_CENTER,
                        "ПРОДАЖА",
                        FontId::new(8.0 * s, theme::bold()),
                        theme::GOLD,
                    );
                    match self.sell_drag {
                        // Над зоной — за сколько продастся (внизу: середину закрывает фигура).
                        Some((stock, _)) if hovered => self.paint_price(
                            painter,
                            board,
                            tex,
                            vec2(center.x, rect.max.y - 14.0),
                            pos.sell_price(stock),
                            theme::GOLD,
                        ),
                        _ => {
                            painter.text(
                                board.art_to_screen(center),
                                Align2::CENTER_CENTER,
                                "перетащите\nфигуру\nиз запаса",
                                FontId::new(6.0 * s, theme::bold()),
                                dim,
                            );
                        }
                    }
                }
            }
        }
        // Фигура, которую несут на продажу, — под курсором.
        if let Some((stock, at)) = self.sell_drag {
            let player = pos.rogue().map_or(Color::White, |r| r.player);
            let size = SPRITE * s * SHOP_PIECE;
            let rect = Rect::from_min_size(board.snap(at - size / 2.0), size);
            self.paint_stock(
                painter,
                tex,
                player,
                stock,
                rect,
                painter.clip_rect(),
                Color32::WHITE,
            );
        }
    }

    /// Вещь магазина или сундука на месте `rect` (пиксели арта): фигура — уменьшенным
    /// спрайтом (с анимацией силы), герб — картинкой.
    fn paint_ware(
        &self,
        painter: &Painter,
        board: &BoardGeom,
        tex: &Textures,
        rect: Rect,
        ware: Ware,
        tint: Color32,
    ) {
        let s = board.scale;
        let center = rect.center().to_vec2();
        match ware {
            Ware::Piece(stock) => {
                let player = self
                    .game
                    .position()
                    .rogue()
                    .map_or(Color::White, |r| r.player);
                let ground = board.art_to_screen(vec2(center.x, rect.min.y + SHOP_GROUND));
                let sprite = Rect::from_min_size(
                    board.snap(ground - ANCHOR * s * SHOP_PIECE),
                    SPRITE * s * SHOP_PIECE,
                );
                let cell = Rect::from_min_max(
                    board.art_to_screen(rect.min.to_vec2()),
                    board.art_to_screen(rect.max.to_vec2()),
                )
                .shrink(s);
                self.paint_stock(painter, tex, player, stock, sprite, cell, tint);
            }
            Ware::Crest(crest) => {
                let picture = Rect::from_min_size(
                    board.snap(board.art_to_screen(vec2(
                        center.x - (CREST.x / 2.0).floor(),
                        rect.min.y + CREST_TOP,
                    ))),
                    CREST * s,
                );
                painter.image(tex.ui.crest(crest).id(), picture, UV, tint);
                self.paint_crest_badge(painter, tex, picture, crest);
            }
            Ware::Medal(kind) => {
                let picture =
                    Rect::from_min_size(
                        board.snap(board.art_to_screen(vec2(
                            center.x - (MEDAL.x / 2.0).floor(),
                            rect.min.y + 6.0,
                        ))),
                        MEDAL * s,
                    );
                painter.image(tex.ui.medal(kind).id(), picture, UV, tint);
            }
            Ware::Card(card) => {
                // Карта во всё место: в сундуке её не сдвигают вниз (`Pick` — на 8).
                let picture =
                    Rect::from_min_size(
                        board.snap(board.art_to_screen(vec2(
                            center.x - (POWER_CARD.x / 2.0).floor(),
                            rect.min.y,
                        ))),
                        POWER_CARD * s,
                    );
                painter.image(tex.ui.rogue_card(card).id(), picture, UV, tint);
            }
        }
    }

    // ----- Rogue-like: карты сил в руке -----

    /// Карты в руке (место, карта, прямоугольник в пикселях арта): в левом верхнем углу поля,
    /// выбранная приподнята.
    pub(super) fn hand_slots(&self) -> Vec<(usize, Card, Rect)> {
        self.game
            .position()
            .hand()
            .into_iter()
            .enumerate()
            .map(|(i, card)| {
                let lift = if self.power_pick == Some(i) {
                    POWER_HAND_LIFT
                } else {
                    0.0
                };
                let min = POWER_HAND_AT + vec2(i as f32 * (POWER_CARD.x + POWER_HAND_GAP), -lift);
                (i, card, Rect::from_min_size(min.to_pos2(), POWER_CARD))
            })
            .collect()
    }

    /// Меню карты в магазине (с 0.126): плашки «Сыграть» и «Продать» под выбранной картой,
    /// пиксели арта.
    pub(super) fn card_menu_spots(&self) -> Vec<(CardMenu, Rect)> {
        let Some((.., card)) = self
            .card_menu
            .and_then(|slot| self.hand_slots().into_iter().find(|&(i, ..)| i == slot))
        else {
            return Vec::new();
        };
        let at = Pos2::new(card.min.x, card.max.y + 3.0);
        vec![
            (CardMenu::Play, Rect::from_min_size(at, CARD_MENU)),
            (
                CardMenu::Sell,
                Rect::from_min_size(at + vec2(0.0, CARD_MENU.y + 2.0), CARD_MENU),
            ),
        ]
    }

    /// Плашка меню карты под курсором.
    pub(super) fn card_menu_under(&self, board: &BoardGeom, pointer: Pos2) -> Option<CardMenu> {
        let at = board.to_art(pointer).to_pos2();
        self.card_menu_spots()
            .into_iter()
            .find(|(_, rect)| rect.contains(at))
            .map(|(item, _)| item)
    }

    /// Можно ли нажать плашку меню карты: сыграть — если карта готова, продать — пока можно
    /// продавать.
    pub(super) fn card_menu_usable(&self, item: CardMenu) -> bool {
        match item {
            CardMenu::Play => self.card_menu.is_some_and(|slot| self.hand_usable(slot)),
            CardMenu::Sell => self.game.position().can_sell(),
        }
    }

    /// Меню карты: «Сыграть» и «Продать» с ценой.
    fn paint_card_menu(&self, painter: &Painter, board: &BoardGeom, tex: &Textures) {
        let s = board.scale;
        let dim = Color32::from_rgb(0x6c, 0x76, 0x8a);
        for (item, rect) in self.card_menu_spots() {
            let usable = self.card_menu_usable(item);
            let hovered = usable && self.card_menu_hover == Some(item);
            let edge = if hovered {
                Color32::from_rgb(0xff, 0xe2, 0x7a)
            } else if usable {
                Color32::from_rgb(0xd0, 0x7d, 0x1c)
            } else {
                Color32::from_rgb(0x2a, 0x33, 0x45)
            };
            self.shop_plate(painter, board, rect, hovered, edge);
            let center = rect.center().to_vec2();
            let color = if usable { theme::TEXT_BRIGHT } else { dim };
            match item {
                CardMenu::Play => painter.text(
                    board.art_to_screen(center),
                    Align2::CENTER_CENTER,
                    "Сыграть",
                    FontId::new(7.0 * s, theme::bold()),
                    color,
                ),
                CardMenu::Sell => {
                    painter.text(
                        board.art_to_screen(vec2(rect.min.x + 23.0, center.y)),
                        Align2::CENTER_CENTER,
                        "Продать",
                        FontId::new(7.0 * s, theme::bold()),
                        color,
                    );
                    self.paint_price(
                        painter,
                        board,
                        tex,
                        vec2(rect.max.x - 16.0, center.y),
                        card_sell_price(),
                        theme::GOLD,
                    );
                    continue;
                }
            };
        }
    }

    /// Медали игрока (с 0.126): вид, сколько, место в пикселях арта — в ряд у правого верхнего
    /// угла поля, по порядку видов.
    pub(super) fn medal_row(&self) -> Vec<(Kind, u8, Rect)> {
        let medals = self.game.position().medals();
        let owned: Vec<(Kind, u8)> = Kind::ALL
            .into_iter()
            .map(|kind| (kind, medals[kind as usize]))
            .filter(|&(_, n)| n > 0)
            .collect();
        let n = owned.len() as f32;
        let left = MEDALS_AT.x - n * (MEDAL.x + MEDAL_GAP) + MEDAL_GAP;
        owned
            .into_iter()
            .enumerate()
            .map(|(i, (kind, count))| {
                let min = vec2(left + i as f32 * (MEDAL.x + MEDAL_GAP), MEDALS_AT.y);
                (kind, count, Rect::from_min_size(min.to_pos2(), MEDAL))
            })
            .collect()
    }

    /// Медали игрока и их число «×N» под каждой.
    fn paint_medals(&self, painter: &Painter, board: &BoardGeom, tex: &Textures) {
        let s = board.scale;
        for (kind, count, rect) in self.medal_row() {
            let picture = Rect::from_min_size(
                board.snap(board.art_to_screen(rect.min.to_vec2())),
                MEDAL * s,
            );
            painter.image(tex.ui.medal(kind).id(), picture, UV, Color32::WHITE);
            let text = paint(&Glyph::text(&format!("×{count}")), theme::GOLD);
            let width = self.text_width(&text) * s;
            let corner = board.art_to_screen(vec2(rect.center().x, rect.max.y + 2.0))
                - vec2(width / 2.0, 0.0);
            self.pixel_text(painter, tex, corner, s, &text);
        }
    }

    /// Карта руки под курсором.
    pub(super) fn hand_under(&self, board: &BoardGeom, pointer: Pos2) -> Option<usize> {
        let at = board.to_art(pointer).to_pos2();
        self.hand_slots()
            .into_iter()
            .find(|(.., rect)| rect.contains(at))
            .map(|(i, ..)| i)
    }

    /// Карту с места `slot` можно сыграть сейчас (`Position::card_ready`) — и магазин её ждёт.
    pub(super) fn hand_usable(&self, slot: usize) -> bool {
        let pos = self.game.position();
        self.human_to_move()
            && (!pos.shop_open() || self.shop_ready())
            && pos
                .hand()
                .get(slot)
                .is_some_and(|&card| pos.card_ready(card))
    }

    /// Рука карт сил: тень, карта (нельзя сыграть — притушена), выбранная — в золотой рамке,
    /// под курсором — в светлой.
    fn paint_hand(&self, painter: &Painter, board: &BoardGeom, tex: &Textures) {
        let s = board.scale;
        for (i, card, rect) in self.hand_slots() {
            let screen = Rect::from_min_size(
                board.snap(board.art_to_screen(rect.min.to_vec2())),
                POWER_CARD * s,
            );
            let usable = self.hand_usable(i);
            painter.rect_filled(
                screen.translate(vec2(2.0, 3.0) * s),
                CornerRadius::ZERO,
                Color32::from_black_alpha(77),
            );
            let tint = if usable {
                Color32::WHITE
            } else {
                Color32::from_gray(150)
            };
            painter.image(tex.ui.rogue_card(card).id(), screen, UV, tint);
            let edge = if self.power_pick == Some(i) {
                Some(Color32::from_rgb(0xff, 0xe2, 0x7a))
            } else if usable && self.power_hover == Some(i) {
                Some(Color32::from_rgb(0xd0, 0x7d, 0x1c))
            } else {
                None
            };
            if let Some(edge) = edge {
                painter.rect_stroke(
                    screen.expand(s),
                    CornerRadius::ZERO,
                    Stroke::new(s, edge),
                    StrokeKind::Inside,
                );
            }
        }
    }

    // ----- Кнопка «Конец хода» -----

    /// Курсор над кнопкой «Конец хода».
    pub(super) fn over_button(&self, board: &BoardGeom, pointer: Pos2) -> bool {
        button_rect().contains(board.to_art(pointer).to_pos2())
    }

    /// Кнопка работает, когда человек сходил фигурой и может закончить ход.
    pub(super) fn button_active(&self) -> bool {
        self.human_to_move()
            && self.game.can_end_turn()
            && self.aiming.is_none()
            && self.promotion.is_none()
    }

    fn button_look(&self) -> Button {
        if !self.human_to_move() || self.game.position().shop_open() {
            Button::Disabled
        } else if !self.button_active() {
            Button::Normal
        } else if self.button_down && self.button_hover {
            Button::Pressed
        } else if self.button_hover {
            Button::Hover
        } else if pos_ready(self.game.position(), self.game.legal_moves().is_empty()) {
            // Фигурой уже сходили (или ходить нечем, или расстановка) — пора заканчивать ход.
            Button::Ready
        } else {
            // С 0.111 закончить ход можно и не сходив — кнопка просто не горит.
            Button::Normal
        }
    }

    // ----- Рисование -----

    /// Колоды, кнопка, руки, летящие, возвращающиеся и горящие карты.
    pub(super) fn paint_table(
        &self,
        painter: &Painter,
        board: &BoardGeom,
        tex: &Textures,
        now: f64,
    ) {
        let pos = self.game.position();
        let card = |pic: CardPic, center: Vec2, degrees: f32| {
            self.paint_card(painter, board, tex, pic, center, degrees, false);
        };

        let look = self.button_look();
        let size = if look == Button::Pressed {
            BUTTON + vec2(1.0, 1.0)
        } else {
            BUTTON + SHADOW_SHIFT
        };
        let rect = Rect::from_min_size(board.art_to_screen(BUTTON_AT), size * board.scale);
        let start = pos.planning();
        painter.image(tex.ui.button(look, start).id(), rect, UV, Color32::WHITE);
        self.paint_moves_left(painter, board, tex);
        // Доска под магазином притушена; запас и золото — нет: видно, что куплено. Свёрнут — нет.
        if self.shop_shown() {
            painter.rect_filled(
                board.area,
                CornerRadius::ZERO,
                Color32::from_black_alpha(110),
            );
        }
        self.paint_reserve(painter, board, tex);
        self.paint_hand(painter, board, tex);
        self.paint_gold(painter, board, tex);
        self.paint_mult(painter, board, tex);
        self.paint_medals(painter, board, tex);
        self.paint_crests(painter, board, tex);
        self.paint_shop(painter, board, tex);
        self.paint_card_menu(painter, board, tex);
        self.paint_crest_tip(painter, board);

        for seat in [Seat::Near, Seat::Far].into_iter().filter(|_| SHOW_CARDS) {
            let color = self.seat_color(seat);
            let flying = self.draws.iter().filter(|d| d.color == color).count() as u8;
            if let Some(deck) = tex.cards.deck(color, pos.cards(color).deck_len() + flying) {
                put(
                    painter,
                    board,
                    deck,
                    seat.deck_at() + deck.size / 2.0,
                    Vec2::ZERO,
                    1.0,
                );
            }
            self.paint_points(painter, board, tex, seat);
            let layout = self.hand_layout(seat);
            let n = layout.len();
            let hand = self.hand(seat);
            let hovered = self
                .card_hover
                .filter(|&(s, i)| s == seat && layout.contains(&i))
                .map(|(_, i)| i);
            for k in draw_order(seat, n) {
                let index = layout[k];
                if Some(index) == hovered
                    || self.returning_card(seat, index)
                    || self.aimed_card(seat, index)
                {
                    continue;
                }
                let slot = seat.slot(k, n, 0.0);
                card(
                    self.pic(seat, hand[index], false),
                    slot.center,
                    slot.degrees,
                );
            }
            // Карта, ждущая цель, стоит над рукой со светлой кромкой.
            if let Some(aim) = self.aiming.as_ref().filter(|a| a.seat == seat) {
                let slot = self.slot_of(seat, aim.index, LIFT);
                card(self.pic(seat, aim.kind, true), slot.center, 0.0);
            }
            if let Some(index) = hovered {
                // Крупная карта — на верхнем слое: у края руки она заходит на правую панель,
                // и та её не обрезает.
                let painter = &painter.ctx().layer_painter(egui::LayerId::new(
                    egui::Order::Foreground,
                    egui::Id::new("board-top"),
                ));
                // Светлая кромка — только у карты, которую можно сыграть (и на неё хватает очков).
                let pic = self.pic(seat, hand[index], self.usable(seat, index));
                let center = self.zoomed_center(seat, index, self.hover_lift(seat, now));
                self.paint_card(painter, board, tex, pic, center, 0.0, true);
                if let CardPic::Front { kind, .. } = pic
                    && kind.description().is_empty()
                {
                    // Карты без текста подписаны над картой.
                    let edge = center - vec2(0.0, seat.sign() * (CARD.y * ZOOM / 2.0 + 3.0));
                    let anchor = if seat == Seat::Near {
                        Align2::CENTER_BOTTOM
                    } else {
                        Align2::CENTER_TOP
                    };
                    paint_title(painter, board.art_to_screen(edge), anchor, kind.title());
                }
            }
        }

        // Карта из колоды летит на крайнее место в руке. Летящие карты — последние в руке.
        for (color, flying) in [Color::White, Color::Black].map(|c| {
            (
                c,
                self.draws
                    .iter()
                    .filter(|d| d.color == c)
                    .collect::<Vec<_>>(),
            )
        }) {
            let seat = self.seat_of(color);
            let hand = self.hand(seat);
            let n = self.hand_layout(seat).len() + 1;
            for (j, draw) in flying.iter().enumerate() {
                let u = ((now - draw.start) / DRAW_TIME).clamp(0.0, 1.0) as f32;
                if u <= 0.0 {
                    continue;
                }
                let from = seat.deck_at() + CARD / 2.0 + vec2(2.0, 0.0);
                let k = draw_order(seat, n).last().unwrap_or(0);
                let to = seat.slot(k, n, 0.0).center;
                let index = hand.len() - flying.len() + j;
                let kind = hand.get(index).copied().unwrap_or(CardKind::Normal);
                card(
                    self.pic(seat, kind, false),
                    from + (to - from) * ease_out(u),
                    0.0,
                );
            }
        }

        // Отпущенная над рукой карта возвращается на своё место и ложится в веер.
        for back in &self.returning {
            let u = ((now - back.start) / RETURN_TIME).clamp(0.0, 1.0) as f32;
            let slot = self.slot_of(back.seat, back.index, 0.0);
            let e = ease_in_out(u);
            let at = back.from + (slot.center - back.from) * e;
            let degrees = (back.tilt + (slot.degrees - back.tilt) * e).round();
            card(self.pic(back.seat, back.kind, false), at, degrees);
        }

        for burn in &self.burns {
            let frame = ((now - burn.start) / BURN_FRAME_TIME) as usize;
            let min = burn.center - CARD / 2.0 - BURN_CARD_AT;
            let rect = Rect::from_min_size(
                board.snap(board.art_to_screen(min)),
                BURN_FRAME * board.scale,
            );
            let texture = tex.cards.burn(burn.kind, burn.color, frame);
            painter.image(texture.id(), rect, UV, Color32::WHITE);
            // Текст и цена гаснут, пока огонь не добрался до середины карты.
            let fade = 1.0 - ((now - burn.start) / BURN_TEXT_TIME) as f32;
            let at = board.art_to_screen(burn.center);
            paint_card_text(painter, at, board.scale, 0.0, burn.kind, fade.max(0.0));
            if let Some((points, n)) = burn.kind.cost()
                && fade > 0.0
            {
                let tint = COST_TINT[points as usize];
                let tint = Color32::from_rgb((tint >> 16) as u8, (tint >> 8) as u8, tint as u8)
                    .gamma_multiply(fade);
                let text: Vec<(Glyph, Color32)> = Glyph::digits(n.into())
                    .into_iter()
                    .map(|g| (g, tint))
                    .collect();
                let corner = burn.center - CARD / 2.0 + cost_corner(points, n);
                self.pixel_text(
                    painter,
                    tex,
                    board.art_to_screen(corner),
                    board.scale,
                    &text,
                );
            }
        }
    }

    /// Карта с тенью и текстом; `zoomed` — в `ZOOM` раз крупнее, для карты под курсором.
    #[allow(clippy::too_many_arguments)]
    fn paint_card(
        &self,
        painter: &Painter,
        board: &BoardGeom,
        tex: &Textures,
        pic: CardPic,
        center: Vec2,
        degrees: f32,
        zoomed: bool,
    ) {
        if zoomed {
            let sprite = self.art.card_zoomed(painter.ctx(), tex, pic, ZOOM as usize);
            put(painter, board, &sprite, center, SHADOW_SHIFT, ZOOM);
        } else {
            let sprite = self.art.card(painter.ctx(), tex, pic, degrees);
            put(painter, board, &sprite, center, Vec2::ZERO, 1.0);
        }
        if let CardPic::Front { kind, .. } = pic {
            let zoom = if zoomed { ZOOM } else { 1.0 };
            paint_card_text(
                painter,
                board.art_to_screen(center),
                board.scale * zoom,
                degrees,
                kind,
                1.0,
            );
        }
    }

    /// Взятая карта — поверх всего: доски, фигур и панелей.
    pub(super) fn paint_held(&self, painter: &Painter, board: &BoardGeom, tex: &Textures) {
        let Some(held) = &self.held else {
            return;
        };
        let pic = self.pic(held.seat, held.kind, true);
        let degrees = held.tilt.round();
        let sprite = self.art.card(painter.ctx(), tex, pic, degrees);
        put(
            painter,
            board,
            &sprite,
            held.at,
            HELD_SHADOW - SHADOW_SHIFT,
            1.0,
        );
        paint_card_text(
            painter,
            board.art_to_screen(held.at),
            board.scale,
            degrees,
            held.kind,
            1.0,
        );
    }

    /// Стрелка карты, ждущей цель, — от своего короля к курсору, как в Hearthstone от
    /// героя; над своей фигурой она голубая и вокруг клетки встают метки цели. У обмена,
    /// когда первая фигура выбрана, метки остаются на ней, а стрелка идёт от неё ко
    /// второй — к той, с кем она поменяется.
    pub(super) fn paint_aim(
        &self,
        painter: &Painter,
        board: &BoardGeom,
        tex: &Textures,
        pointer: Option<Pos2>,
    ) {
        let Some(aim) = &self.aiming else {
            return;
        };
        let pos = self.game.position();
        let color = self.seat_color(aim.seat);
        // Метки цели — четыре шеврона вокруг ромба клетки.
        let mark = |sq: Square, look: Aim| {
            let ground = board.to_art(board.ground(sq));
            let min = ground - TILE / 2.0 - Vec2::splat(TARGET_PAD);
            let size = TILE + Vec2::splat(2.0 * TARGET_PAD);
            let rect = Rect::from_min_size(board.art_to_screen(min), size * board.scale);
            painter.image(tex.ui.target(look).id(), rect, UV, Color32::WHITE);
            ground - vec2(0.0, AIM_RISE)
        };
        if let Some(first) = aim.first {
            mark(first, Aim::Ally);
        }
        // Стрелка выходит из макушки короля или первой фигуры обмена (без короля карты не
        // играют — и целиться нечем).
        let Some(origin) = aim.first.or(pos.king_square(color)) else {
            return;
        };
        let height = pos
            .piece(origin)
            .map_or(0.0, |p| self.art.height(p.color, p.kind));
        let start = board.to_art(board.ground(origin)) - vec2(0.0, height + KING_CROWN);
        let locked = self.hovered.filter(|sq| self.aim_targets().contains(sq));
        let (end, look) = match (locked, pointer) {
            (Some(target), _) => {
                // Карта по чужой фигуре — красная стрелка и метка.
                let look = if pos.piece(target).is_some_and(|p| p.color != color) {
                    Aim::Enemy
                } else {
                    Aim::Ally
                };
                (mark(target, look), look)
            }
            (None, Some(pointer)) => (board.to_art(pointer), Aim::Idle),
            (None, None) => return,
        };
        // Цель — сама фигура, от которой идёт стрелка: хватает меток цели.
        if locked != Some(origin) {
            self.paint_arrow(painter, board, tex, start, end, look);
        }
    }

    /// Стрелка из звеньев по квадратичной кривой: выходит вверх и подходит к цели
    /// сверху-сбоку; у начала звенья мелкие, к цели крупнее, на конце наконечник.
    fn paint_arrow(
        &self,
        painter: &Painter,
        board: &BoardGeom,
        tex: &Textures,
        start: Vec2,
        end: Vec2,
        look: Aim,
    ) {
        const SAMPLES: usize = 160;
        let control = vec2(start.x, start.y.min(end.y) - 0.28 * (end - start).length());
        let points: Vec<Vec2> = (0..=SAMPLES)
            .map(|i| {
                let t = i as f32 / SAMPLES as f32;
                start * (1.0 - t) * (1.0 - t) + control * 2.0 * (1.0 - t) * t + end * t * t
            })
            .collect();
        let mut lengths = vec![0.0f32];
        for pair in points.windows(2) {
            lengths.push(lengths.last().copied().unwrap_or(0.0) + (pair[1] - pair[0]).length());
        }
        let total = lengths.last().copied().unwrap_or(0.0);
        // Точка и направление касательной на расстоянии `d` вдоль кривой.
        let point_at = |d: f32| {
            let i = lengths.partition_point(|&s| s < d).clamp(1, SAMPLES);
            let dir = points[i] - points[i - 1];
            (points[i], dir.y.atan2(dir.x).to_degrees())
        };
        let head = ARROW_HEAD * 0.8;
        let mut parts = Vec::new();
        let mut d = 6.0;
        while d < total - head {
            let frac = d / total.max(1.0);
            let (part, step) = if frac < 0.3 {
                (ArrowPart::Small, ARROW_SPACING[0])
            } else if frac < 0.62 {
                (ArrowPart::Medium, ARROW_SPACING[1])
            } else {
                (ArrowPart::Large, ARROW_SPACING[2])
            };
            parts.push((part, point_at(d)));
            d += step;
        }
        parts.push((ArrowPart::Head, point_at((total - head / 2.0).max(0.0))));
        let sprites: Vec<(Vec2, Sprite)> = parts
            .into_iter()
            .map(|(part, (at, degrees))| {
                let degrees = (degrees / 5.0).round() as i32 * 5;
                (at, self.art.arrow(painter.ctx(), tex, part, look, degrees))
            })
            .collect();
        // Сначала все тени, потом звенья от карты к наконечнику.
        for (at, sprite) in &sprites {
            let min = board.snap(board.art_to_screen(*at - sprite.size / 2.0));
            let rect = Rect::from_min_size(min, (sprite.size + SHADOW_SHIFT) * board.scale);
            painter.image(sprite.shadow.id(), rect, UV, Color32::WHITE);
        }
        for (at, sprite) in &sprites {
            let min = board.snap(board.art_to_screen(*at - sprite.size / 2.0));
            let rect = Rect::from_min_size(min, sprite.size * board.scale);
            painter.image(sprite.body.id(), rect, UV, Color32::WHITE);
        }
    }
}

/// Название и описание на лице карты: название — в шапке справа от кристалла,
/// описание — в нижнем поле. Текст подбирается по размеру поля и поворачивается с картой;
/// `px` — точек экрана на пиксель карты.
pub(super) fn paint_card_text(
    painter: &Painter,
    center: Pos2,
    px: f32,
    degrees: f32,
    kind: CardKind,
    alpha: f32,
) {
    let description = kind.description();
    if description.is_empty() || alpha <= 0.0 {
        return;
    }
    let angle = degrees.to_radians();
    let (sin, cos) = angle.sin_cos();
    let turn = |v: Vec2| vec2(cos * v.x - sin * v.y, sin * v.x + cos * v.y);
    // На тёмных лицах сторон — белый текст; на светлом лице карты победы — багровое
    // название и почти чёрное описание (цвета рамки из `CARD_TYPES` в breach_cards.py).
    let (title_color, text_color) = match kind.family() {
        Family::Victory => (0x8a1426, 0x24060c),
        _ => (0xffffff, 0xffffff),
    };
    // Поля на лице карты 50×70 (пиксели арта) — от её середины: шапка x 17…45, y 4…12,
    // нижнее поле x 6…43, y 45…61 (по рамкам из breach_cards_hd.py; у всех карт одни).
    // Короткое описание не раздувается: на всех картах оно примерно одного размера.
    let boxes = [
        (
            kind.title(),
            vec2(6.0, -27.0),
            vec2(27.0, 8.0),
            f32::INFINITY,
            theme::bold(),
            title_color,
        ),
        (
            &description,
            vec2(-0.5, 18.0),
            vec2(36.0, 15.0),
            DESCRIPTION_FONT,
            theme::semibold(),
            text_color,
        ),
    ];
    for (text, middle, size, cap, family, color) in boxes {
        let largest = (size.y * 0.9).min(size.x * 0.25).min(cap) * px;
        let Some(galley) = fit_text(painter, text, size * px, largest, family) else {
            continue;
        };
        let color = Color32::from_rgb((color >> 16) as u8, (color >> 8) as u8, color as u8)
            .gamma_multiply(alpha);
        let corner = turn(middle * px - galley.size() / 2.0);
        painter.add(TextShape::new(center + corner, galley, color).with_angle(angle));
    }
}

/// Самый крупный шрифт не больше `largest`, которым текст влезает в поле `size` (с
/// переносом по ширине).
fn fit_text(
    painter: &Painter,
    text: &str,
    size: Vec2,
    largest: f32,
    family: FontFamily,
) -> Option<std::sync::Arc<egui::Galley>> {
    let mut font = largest;
    while font >= 4.0 {
        // Цвет не запекается в разметку: его задаёт `TextShape` (и гасит при сгорании).
        let galley = painter.layout(
            text.to_owned(),
            FontId::new(font, family.clone()),
            Color32::PLACEHOLDER,
            size.x,
        );
        if galley.size().x <= size.x + 0.5 && galley.size().y <= size.y + 0.5 {
            return Some(galley);
        }
        font *= 0.92;
    }
    None
}

/// Подпись над картой: светлый текст на тёмно-синей плашке с кромкой, как панели ITB.
pub(super) fn paint_title(painter: &Painter, at: Pos2, anchor: Align2, text: &str) {
    let galley = painter.layout_no_wrap(
        text.to_owned(),
        FontId::new(13.0, theme::bold()),
        theme::TEXT_BRIGHT,
    );
    let rect = anchor
        .anchor_size(at, galley.size())
        .expand2(vec2(6.0, 2.0));
    painter.rect_filled(
        rect,
        CornerRadius::ZERO,
        Color32::from_rgb(0x18, 0x1c, 0x29),
    );
    painter.rect_stroke(
        rect,
        CornerRadius::ZERO,
        Stroke::new(1.0_f32, Color32::from_rgb(0x49, 0x5c, 0x79)),
        StrokeKind::Inside,
    );
    painter.galley(rect.min + vec2(6.0, 2.0), galley, theme::TEXT_BRIGHT);
}

/// Вариант снега и глыбы на клетке — у соседних клеток разные (`variant_of` в
/// breach_freeze.py).
pub(super) fn ice_variant(sq: Square) -> usize {
    ((file_of(sq) * 3 + rank_of(sq) * 5) % 4) as usize
}

/// Кадр заморозки на клетке `sq`: в кадре центр верхней грани клетки — `FREEZE_CENTER`.
fn put_frame(painter: &Painter, board: &BoardGeom, sprite: &FrameSprite, sq: Square, alpha: f32) {
    let ground = board.to_art(board.ground(sq));
    let min = board.art_to_screen(ground - FREEZE_CENTER + sprite.at);
    let rect = Rect::from_min_size(min, sprite.size * board.scale);
    let tint = Color32::from_white_alpha((alpha.clamp(0.0, 1.0) * 255.0) as u8);
    painter.image(sprite.texture, rect, UV, tint);
}

/// Картинка с тенью, увеличенная в `zoom` раз; `lift` отодвигает тень дальше обычного —
/// карта поднята над столом.
fn put(painter: &Painter, board: &BoardGeom, sprite: &Sprite, center: Vec2, lift: Vec2, zoom: f32) {
    let size = sprite.size * zoom;
    // Левый верхний угол — на физическом пикселе: карта в веере и в полёте не размывается.
    let min = board.snap(board.art_to_screen(center - size / 2.0));
    let shadow = Rect::from_min_size(
        min + lift * board.scale,
        (size + SHADOW_SHIFT * zoom) * board.scale,
    );
    painter.image(sprite.shadow.id(), shadow, UV, Color32::WHITE);
    let body = Rect::from_min_size(min, size * board.scale);
    painter.image(sprite.body.id(), body, UV, Color32::WHITE);
}

/// В каком порядке рисовать карты руки: правые своей руки поверх левых, у соперника
/// наоборот (рука повёрнута на 180°). Последней в порядке лежит верхняя карта.
fn draw_order(seat: Seat, n: usize) -> Box<dyn DoubleEndedIterator<Item = usize>> {
    match seat {
        Seat::Near => Box::new(0..n),
        Seat::Far => Box::new((0..n).rev()),
    }
}

/// Точка (в пикселях арта) в системе координат карты на месте `slot`: x — поперёк карты
/// от её оси, y — вдоль.
fn card_local(slot: Slot, at: Vec2) -> Vec2 {
    let (sin, cos) = slot.degrees.to_radians().sin_cos();
    let d = at - slot.center;
    // Поворот на тот же угол обратно — в систему координат карты.
    vec2(cos * d.x + sin * d.y, -sin * d.x + cos * d.y)
}

/// Попадает ли точка (в пикселях арта) в карту на месте `slot`.
fn covers(slot: Slot, at: Vec2) -> bool {
    let local = card_local(slot, at);
    local.x.abs() <= CARD.x / 2.0 && local.y.abs() <= CARD.y / 2.0
}

/// Место в веере из `n` карт под точкой `at` (`away` — карты, которых в веере сейчас
/// нет). Из карт, которые накрывают точку, берётся та, к чьей оси точка ближе: полоса
/// каждой карты — от середины до середины между ней и соседями, уже самой карты, и
/// курсор, ведомый вдоль руки, перебирает карты ровно по одной, без перескоков.
fn fan_slot_at(seat: Seat, n: usize, at: Vec2, away: impl Fn(usize) -> bool) -> Option<usize> {
    (0..n)
        .filter(|&k| !away(k))
        .map(|k| (k, seat.slot(k, n, 0.0)))
        .filter(|&(_, slot)| covers(slot, at))
        .min_by(|a, b| {
            let off = |slot: Slot| card_local(slot, at).x.abs();
            off(a.1).total_cmp(&off(b.1))
        })
        .map(|(k, _)| k)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_matches_the_art_layout() {
        for seat in [Seat::Near, Seat::Far] {
            for n in 1..=7 {
                for k in 0..n {
                    for lift in [0.0, LIFT] {
                        let c = seat.slot(k, n, lift).center;
                        // Повёрнутая карта занимает не больше квадрата со стороной диагонали.
                        let half = CARD.length() / 2.0;
                        assert!(
                            c.x - half >= TABLE_MIN.x - 12.0 && c.x + half <= TABLE_MAX.x + 12.0
                        );
                        assert!(
                            c.y - half >= TABLE_MIN.y - 12.0 && c.y + half <= TABLE_MAX.y + 12.0
                        );
                    }
                }
            }
            let deck = seat.deck_at();
            assert!(deck.x >= TABLE_MIN.x && deck.x + DECK_FULL.x <= TABLE_MAX.x);
            assert!(deck.y >= TABLE_MIN.y && deck.y + DECK_FULL.y <= TABLE_MAX.y);
        }
        // Как в table_preview: своя колода в (24, 390) холста, чужая — в (570, 1), кнопка —
        // в (572, 231); холст сдвинут от картинки доски на (100, 70).
        let canvas = vec2(100.0, 70.0);
        assert_eq!(Seat::Near.deck_at() + canvas, vec2(24.0, 390.0));
        assert_eq!(Seat::Far.deck_at() + canvas, vec2(570.0, 1.0));
        assert_eq!(BUTTON_AT + canvas, vec2(572.0, 231.0));
        assert!(button_rect().max.x <= TABLE_MAX.x);
    }

    #[test]
    fn far_hand_mirrors_the_near_one() {
        for k in 0..5 {
            let near = Seat::Near.slot(k, 5, 0.0);
            let far = Seat::Far.slot(4 - k, 5, 0.0);
            // Через центр доски: карта k своей руки переходит в карту n−1−k соперника.
            assert!((near.center + far.center - MID * 2.0).length() < 1e-3);
            assert!((near.degrees - far.degrees).abs() < 1e-3);
        }
    }

    #[test]
    fn card_hit_test_follows_the_rotation() {
        let slot = Slot {
            center: vec2(100.0, 100.0),
            degrees: 20.0,
        };
        assert!(covers(slot, slot.center));
        // Верхний край повёрнутой по часовой карты сдвинут вправо.
        let top = vec2(20f32.to_radians().sin(), -20f32.to_radians().cos()) * 33.0;
        assert!(covers(slot, slot.center + top));
        assert!(!covers(slot, slot.center + vec2(0.0, -40.0)));
    }

    #[test]
    fn scanning_the_hand_visits_every_card_in_turn() {
        for seat in [Seat::Near, Seat::Far] {
            let sign = seat.sign();
            let base = seat.hand_at();
            for n in 1..=7 {
                let step = if n > 1 {
                    STEP.min(SPREAD / (n - 1) as f32)
                } else {
                    STEP
                };
                let angle = |k: usize| (k as f32 - (n as f32 - 1.0) / 2.0) * step;
                // Точка на дуге веера, по которой стоят середины карт.
                let arc = |degrees: f32| {
                    let (sin, cos) = degrees.to_radians().sin_cos();
                    vec2(base.x + sin * RADIUS, base.y + sign * RADIUS * (1.0 - cos))
                };
                assert!((arc(angle(0)) - seat.slot(0, n, 0.0).center).length() < 1e-3);
                // Курсор идёт вдоль руки: каждая карта — один раз и по порядку.
                let mut seen: Vec<usize> = Vec::new();
                let mut degrees = angle(0) - 12.0;
                while degrees < angle(n - 1) + 12.0 {
                    if let Some(k) = fan_slot_at(seat, n, arc(degrees), |_| false)
                        && seen.last() != Some(&k)
                    {
                        seen.push(k);
                    }
                    degrees += 0.1;
                }
                assert_eq!(seen, (0..n).collect::<Vec<_>>(), "{seat:?}, {n} карт");
                // Граница между соседями — ровно посередине между их осями.
                for k in 0..n - 1 {
                    let middle = (angle(k) + angle(k + 1)) / 2.0;
                    assert_eq!(fan_slot_at(seat, n, arc(middle - 0.2), |_| false), Some(k));
                    assert_eq!(
                        fan_slot_at(seat, n, arc(middle + 0.2), |_| false),
                        Some(k + 1)
                    );
                }
            }
        }
        // Карты, которой нет в веере, курсор не находит: под ним — соседняя.
        let middle = Seat::Near.slot(2, 5, 0.0).center;
        let left = middle - vec2(10.0, 0.0);
        assert_eq!(fan_slot_at(Seat::Near, 5, left, |_| false), Some(2));
        assert_eq!(fan_slot_at(Seat::Near, 5, left, |k| k == 2), Some(1));
        // Над веером (там, где крупная карта) — ни одной.
        let above = middle - vec2(0.0, CARD.y / 2.0 + 8.0);
        assert_eq!(fan_slot_at(Seat::Near, 5, above, |_| false), None);
    }

    #[test]
    fn hand_zone_covers_the_fan_but_not_the_board() {
        let zone = Seat::Near.hand_zone(5);
        for k in 0..5 {
            assert!(zone.contains(Seat::Near.slot(k, 5, 0.0).center.to_pos2()));
        }
        // Ниже руки — тоже рука, над доской — нет.
        assert!(zone.contains(HAND_AT.to_pos2() + vec2(0.0, 200.0)));
        assert!(!zone.contains(MID.to_pos2()));
        assert!(!zone.contains((HAND_AT + vec2(0.0, -90.0)).to_pos2()));
        let far = Seat::Far.hand_zone(5);
        assert!(far.contains((MID * 2.0 - HAND_AT).to_pos2()));
        assert!(!far.contains(MID.to_pos2()));
    }
}
