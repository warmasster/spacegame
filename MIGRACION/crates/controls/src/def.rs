//! Controls and indicators as data (`panels/*.jsonc`, see `docs/MANDOS_Y_MAQUINAS.md` §4, §6). One
//! flat record for every kind: each kind reads the fields it understands and the builder refuses
//! the ones that make no sense (unknown fields are refused by serde). Quantities carry units.
use lunar_signals::Q;
use serde::Deserialize;

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bind {
    /// The signal it writes (mechanisms) or reads (indicators).
    #[serde(default)]
    pub senal: Option<String>,
    /// The second axis of a two-axis lever.
    #[serde(default)]
    pub senal_y: Option<String>,
    /// A network joint it opens and closes (breakers, valves): the owner resolves it.
    #[serde(default)]
    pub union: Option<String>,
    /// One mechanism with a handle in several places (a valve's wheel on each side of its
    /// bulkhead): the controls that share its signal move together.
    #[serde(default)]
    pub comun: bool,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Requires {
    /// Power it draws to work (lamps, displays: none without it).
    #[serde(default)]
    pub energia: Option<Q>,
    /// A crew role.
    #[serde(default)]
    pub rol: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fixed {
    /// Position of its centre in the panel (mm from the bottom-left corner).
    #[serde(default)]
    pub mm: Option<[f32; 2]>,
    /// Grid cell (column, row from the top).
    #[serde(default)]
    pub celda: Option<[u32; 2]>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Accel {
    /// Notches/s from which the step grows.
    pub desde: f64,
    pub k: f64,
    pub gamma: f64,
    pub max: f64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Inertia {
    /// Wheels: moment (arbitrary scale) and damping (1/s). Needles: natural frequency (Hz) and
    /// damping ratio.
    #[serde(default)]
    pub momento: Option<f64>,
    #[serde(default)]
    pub amortiguamiento: Option<f64>,
    #[serde(default)]
    pub frecuencia: Option<f64>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Detent {
    pub en: Q,
    #[serde(default)]
    pub fuerza: Option<f64>,
    #[serde(default)]
    pub ancho: Option<Q>,
    #[serde(default)]
    pub nombre: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GateDef {
    pub en: Q,
    #[serde(default)]
    pub levantar: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LampRule {
    /// Condition (expression).
    pub si: String,
    /// Colour name or sRGB.
    #[serde(default)]
    pub color: Option<ColorDef>,
    /// Blink rate (Hz), 0: steady.
    #[serde(default)]
    pub parpadeo: Option<f64>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
pub enum ColorDef {
    Name(String),
    Rgb([u8; 3]),
}

impl ColorDef {
    pub fn rgb(&self) -> Result<[u8; 3], String> {
        match self {
            ColorDef::Rgb(c) => Ok(*c),
            ColorDef::Name(n) => named_color(n).ok_or_else(|| format!("color desconocido '{n}'")),
        }
    }
}

/// Panel colours by name.
pub fn named_color(n: &str) -> Option<[u8; 3]> {
    Some(match n {
        "verde" => [60, 255, 90],
        "ambar" | "ámbar" => [255, 170, 20],
        "rojo" => [255, 40, 30],
        "blanco" => [255, 250, 235],
        "azul" => [60, 140, 255],
        "cian" => [40, 230, 255],
        "amarillo" => [255, 230, 40],
        "magenta" => [255, 60, 220],
        _ => return None,
    })
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Zones {
    #[serde(default)]
    pub verde: Option<[Q; 2]>,
    #[serde(default, alias = "ámbar")]
    pub ambar: Option<[Q; 2]>,
    #[serde(default)]
    pub roja: Option<[Q; 2]>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Warning {
    pub rotulo: String,
    pub si: String,
    /// 1 caution (amber), 2 warning (red, flashing until acknowledged).
    #[serde(default = "one")]
    pub nivel: u8,
}

fn one() -> u8 {
    1
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Page {
    pub titulo: String,
    /// Lines of text; `{señal}` or `{señal:decimales}` is replaced by the value in its unit,
    /// `{señal|A|B|C}` by the label of its index.
    #[serde(default)]
    pub lineas: Vec<String>,
    /// Multi-function displays: its instruments (`mfd`), and its legend on the bezel.
    #[serde(default)]
    pub elementos: Vec<crate::mfd::WidgetDef>,
    #[serde(default)]
    pub boton: Option<String>,
    /// Made by the ship from what it has ("energia", "aire", "depositos", "motores", "casco",
    /// "puertas"): filled in before it is built.
    #[serde(default)]
    pub auto: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControlDef {
    pub id: String,
    pub kind: String,
    /// Shown in the HUD ("Consigna de temperatura del refrigerante").
    #[serde(default)]
    pub nombre: Option<String>,
    /// Help: what it does and what it needs (manual, HUD).
    #[serde(default)]
    pub ayuda: Option<String>,
    // ---- placement ----
    #[serde(default)]
    pub grupo: Option<String>,
    #[serde(default)]
    pub prioridad: Option<i32>,
    #[serde(default)]
    pub fijo: Option<Fixed>,
    /// Footprint override (mm).
    #[serde(default)]
    pub tamano: Option<[f32; 2]>,
    /// Silkscreen label (above), and where: "arriba" (default) or "abajo".
    #[serde(default)]
    pub rotulo: Option<String>,
    #[serde(default)]
    pub rotulo_en: Option<String>,
    // ---- wiring ----
    #[serde(default)]
    pub bind: Bind,
    #[serde(default)]
    pub requiere: Requires,
    #[serde(default)]
    pub sonido: Option<serde_json::Value>,
    // ---- mechanisms ----
    #[serde(default)]
    pub defecto: Option<Q>,
    #[serde(default)]
    pub posiciones: Vec<String>,
    /// Values per position (else the index).
    #[serde(default)]
    pub valores: Vec<Q>,
    /// Spring-loaded positions: position label → where it springs back to.
    #[serde(default)]
    pub muelle: Option<std::collections::BTreeMap<String, String>>,
    #[serde(default)]
    pub tirar_para_mover: Vec<String>,
    #[serde(default)]
    pub tirar_para_girar: Vec<String>,
    /// Buttons: "momentaneo", "enclavado", "seta", "pulso".
    #[serde(default)]
    pub modo: Option<String>,
    #[serde(default)]
    pub pulso: bool,
    /// Selectors: "tope" or "libre".
    #[serde(default)]
    pub vuelta: Option<String>,
    #[serde(default)]
    pub rango: Option<[Q; 2]>,
    #[serde(default)]
    pub resolucion: Option<Q>,
    #[serde(default)]
    pub paso: Option<Q>,
    #[serde(default)]
    pub paso_grueso: Option<Q>,
    #[serde(default)]
    pub paso_fino: Option<Q>,
    #[serde(default)]
    pub aceleracion: Option<Accel>,
    #[serde(default)]
    pub inercia: Option<Inertia>,
    /// "duros" or "libre".
    #[serde(default)]
    pub topes: Option<String>,
    #[serde(default)]
    pub retenes: Vec<Detent>,
    /// "lineal" or "log".
    #[serde(default)]
    pub curva: Option<String>,
    /// Travel of the knob or lever (degrees).
    #[serde(default)]
    pub recorrido: Option<f64>,
    #[serde(default)]
    pub vueltas: Option<f64>,
    /// Wheels: what a press does: "reset" or "grueso".
    #[serde(default)]
    pub pulsar: Option<String>,
    /// Display unit (and of the HUD), e.g. "°C", "%".
    #[serde(default)]
    pub unidad: Option<String>,
    #[serde(default)]
    pub ejes: Option<u8>,
    #[serde(default)]
    pub compuerta: Option<GateDef>,
    #[serde(default)]
    pub friccion: Option<f64>,
    #[serde(default)]
    pub muelle_al_centro: bool,
    /// Covers: the controls it guards.
    #[serde(default)]
    pub protege: Vec<String>,
    #[serde(default)]
    pub precinto: bool,
    /// Key switches: the key item it asks for.
    #[serde(default)]
    pub llave: Option<String>,
    /// Keypads: the signal ENTER writes, and the input mask ("###.#").
    #[serde(default)]
    pub destino: Option<String>,
    #[serde(default)]
    pub mascara: Option<String>,
    /// Breakers: rated current, and the trip curve ("rapida", "normal", "lenta").
    #[serde(default)]
    pub nominal: Option<Q>,
    #[serde(default)]
    pub curva_disparo: Option<String>,
    // ---- indicators ----
    /// The signal it shows (indicators; mechanisms use `bind`).
    #[serde(default)]
    pub senal: Option<String>,
    #[serde(default)]
    pub reglas: Vec<LampRule>,
    #[serde(default)]
    pub color: Option<ColorDef>,
    #[serde(default)]
    pub escala: Option<[Q; 2]>,
    #[serde(default)]
    pub zonas: Option<Zones>,
    #[serde(default)]
    pub decimales: Option<usize>,
    #[serde(default)]
    pub digitos: Option<usize>,
    #[serde(default)]
    pub paginas: Vec<Page>,
    /// Screens: the signal with the page shown.
    #[serde(default)]
    pub pagina: Option<String>,
    #[serde(default)]
    pub avisos: Vec<Warning>,
    #[serde(default)]
    pub reconocer: Option<String>,
    #[serde(default)]
    pub columnas: Option<u32>,
    /// Illuminated buttons and switches: when its own light is on (expression).
    #[serde(default)]
    pub luz: Option<String>,
    /// Multi-function displays: buttons per side of their bezel.
    #[serde(default)]
    pub botones: Option<u32>,
}

impl ControlDef {
    pub fn label(&self) -> &str {
        self.nombre.as_deref().or(self.rotulo.as_deref()).unwrap_or(&self.id)
    }
}
