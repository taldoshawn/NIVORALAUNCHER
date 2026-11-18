//! Pojav JSON exchange. Expression evaluation is bounded arithmetic, never code execution.
use super::{CustomButton, Placement, Settings};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::{Mutex, OnceLock},
};

fn token(code: i64, glfw: bool) -> Option<String> {
    if code <= 0 {
        return Some(match code {
            0 => String::new(),
            -1 => "keyboard.show".into(),
            -2 => "controls.toggle".into(),
            -3 => "key.mouse.left".into(),
            -4 => "key.mouse.right".into(),
            -5 => "mouse.virtual.toggle".into(),
            -6 => "key.mouse.middle".into(),
            -7 => "mouse.scroll.up".into(),
            -8 => "mouse.scroll.down".into(),
            -9 => "launcher.menu".into(),
            _ => return None,
        });
    }
    Some(if glfw {
        match code {
            65..=90 => format!("key.keyboard.{}", char::from(b'a' + (code - 65) as u8)),
            48..=57 => format!("key.keyboard.{}", code - 48),
            290..=314 => format!("key.keyboard.f{}", code - 289),
            280 => "key.keyboard.caps.lock".into(),
            282 => "key.keyboard.num.lock".into(),
            283 => "key.keyboard.print.screen".into(),
            284 => "key.keyboard.pause".into(),
            320..=329 => format!("key.keyboard.keypad.{}", code - 320),
            330 => "key.keyboard.keypad.period".into(),
            331 => "key.keyboard.keypad.divide".into(),
            332 => "key.keyboard.keypad.multiply".into(),
            333 => "key.keyboard.keypad.subtract".into(),
            334 => "key.keyboard.keypad.add".into(),
            335 => "key.keyboard.keypad.enter".into(),
            336 => "key.keyboard.keypad.equal".into(),
            32 => "key.keyboard.space".into(),
            256 => "key.keyboard.escape".into(),
            257 => "key.keyboard.enter".into(),
            258 => "key.keyboard.tab".into(),
            259 => "key.keyboard.backspace".into(),
            260 => "key.keyboard.insert".into(),
            261 => "key.keyboard.delete".into(),
            262 => "key.keyboard.right".into(),
            263 => "key.keyboard.left".into(),
            264 => "key.keyboard.down".into(),
            265 => "key.keyboard.up".into(),
            266 => "key.keyboard.page.up".into(),
            267 => "key.keyboard.page.down".into(),
            268 => "key.keyboard.home".into(),
            269 => "key.keyboard.end".into(),
            340 => "key.keyboard.left.shift".into(),
            341 => "key.keyboard.left.control".into(),
            342 => "key.keyboard.left.alt".into(),
            343 => "key.keyboard.left.win".into(),
            344 => "key.keyboard.right.shift".into(),
            345 => "key.keyboard.right.control".into(),
            346 => "key.keyboard.right.alt".into(),
            347 => "key.keyboard.right.win".into(),
            39 => "key.keyboard.apostrophe".into(),
            44 => "key.keyboard.comma".into(),
            45 => "key.keyboard.minus".into(),
            46 => "key.keyboard.period".into(),
            47 => "key.keyboard.slash".into(),
            59 => "key.keyboard.semicolon".into(),
            61 => "key.keyboard.equal".into(),
            91 => "key.keyboard.left.bracket".into(),
            92 => "key.keyboard.backslash".into(),
            93 => "key.keyboard.right.bracket".into(),
            96 => "key.keyboard.grave.accent".into(),
            _ => return None,
        }
    } else {
        match code {
            29..=54 => format!("key.keyboard.{}", char::from(b'a' + (code - 29) as u8)),
            7..=16 => format!("key.keyboard.{}", code - 7),
            131..=142 => format!("key.keyboard.f{}", code - 130),
            59 => "key.keyboard.left.shift".into(),
            60 => "key.keyboard.right.shift".into(),
            61 => "key.keyboard.tab".into(),
            62 => "key.keyboard.space".into(),
            66 => "key.keyboard.enter".into(),
            67 => "key.keyboard.backspace".into(),
            111 => "key.keyboard.escape".into(),
            113 => "key.keyboard.left.control".into(),
            _ => return None,
        }
    })
}

#[derive(Clone, Debug)]
enum Op {
    Number(f64),
    Variable(String),
    Add,
    Sub,
    Mul,
    Div,
    Neg,
}
fn compile(text: &str) -> Result<Vec<Op>, String> {
    if text.len() > 8192 {
        return Err("Expressão de posição muito longa".into());
    }
    struct Parser<'a> {
        s: &'a [u8],
        p: usize,
        out: Vec<Op>,
        depth: usize,
    }
    impl Parser<'_> {
        fn space(&mut self) {
            while self.s.get(self.p).is_some_and(u8::is_ascii_whitespace) {
                self.p += 1
            }
        }
        fn sum(&mut self) -> Result<(), String> {
            self.product()?;
            loop {
                self.space();
                let op = match self.s.get(self.p) {
                    Some(b'+') => Op::Add,
                    Some(b'-') => Op::Sub,
                    _ => break,
                };
                self.p += 1;
                self.product()?;
                self.out.push(op);
            }
            Ok(())
        }
        fn product(&mut self) -> Result<(), String> {
            self.atom()?;
            loop {
                self.space();
                let op = match self.s.get(self.p) {
                    Some(b'*') => Op::Mul,
                    Some(b'/') => Op::Div,
                    _ => break,
                };
                self.p += 1;
                self.atom()?;
                self.out.push(op);
            }
            Ok(())
        }
        fn atom(&mut self) -> Result<(), String> {
            self.space();
            self.depth += 1;
            if self.depth > 32 {
                return Err("Expressão muito aninhada".into());
            }
            match self.s.get(self.p) {
                Some(b'-' | b'+') => {
                    let neg = self.s[self.p] == b'-';
                    self.p += 1;
                    self.atom()?;
                    if neg {
                        self.out.push(Op::Neg)
                    }
                }
                Some(b'(') => {
                    self.p += 1;
                    self.sum()?;
                    self.space();
                    if self.s.get(self.p) != Some(&b')') {
                        return Err("Falta ')' na posição".into());
                    }
                    self.p += 1;
                }
                Some(b'$') => {
                    self.p += 1;
                    if self.s.get(self.p) != Some(&b'{') {
                        return Err("Variável inválida".into());
                    }
                    self.p += 1;
                    let start = self.p;
                    while self
                        .s
                        .get(self.p)
                        .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
                    {
                        self.p += 1
                    }
                    if self.s.get(self.p) != Some(&b'}') {
                        return Err("Variável incompleta".into());
                    }
                    let name = std::str::from_utf8(&self.s[start..self.p])
                        .map_err(|_| "Variável inválida")?
                        .to_owned();
                    if ![
                        "screen_width",
                        "screen_height",
                        "width",
                        "height",
                        "margin",
                        "preferred_scale",
                        "right",
                        "bottom",
                    ]
                    .contains(&name.as_str())
                    {
                        return Err(format!("Variável desconhecida: {name}"));
                    }
                    self.out.push(Op::Variable(name));
                    self.p += 1;
                }
                Some(b'p') if self.s.get(self.p..self.p + 3) == Some(b"px(") => {
                    self.p += 3;
                    self.sum()?;
                    self.space();
                    if self.s.get(self.p) != Some(&b')') {
                        return Err("px() incompleto".into());
                    }
                    self.p += 1;
                }
                _ => {
                    let start = self.p;
                    while self
                        .s
                        .get(self.p)
                        .is_some_and(|b| b.is_ascii_digit() || *b == b'.')
                    {
                        self.p += 1
                    }
                    let value = std::str::from_utf8(&self.s[start..self.p])
                        .ok()
                        .and_then(|s| s.parse::<f64>().ok())
                        .filter(|v| v.is_finite())
                        .ok_or("Número ou operação inválida")?;
                    self.out.push(Op::Number(value));
                }
            }
            self.depth -= 1;
            Ok(())
        }
    }
    let mut p = Parser {
        s: text.as_bytes(),
        p: 0,
        out: Vec::new(),
        depth: 0,
    };
    p.sum()?;
    p.space();
    if p.p != text.len() || p.out.len() > 2048 {
        return Err("Expressão não suportada".into());
    }
    Ok(p.out)
}
fn expression(text: &str, vars: &[(&str, f64)]) -> Result<f64, String> {
    static CACHE: OnceLock<Mutex<BTreeMap<String, Vec<Op>>>> = OnceLock::new();
    let mut cache = CACHE
        .get_or_init(Default::default)
        .lock()
        .map_err(|_| "Falha ao avaliar posição")?;
    if !cache.contains_key(text) {
        let ops = compile(text)?;
        if cache.len() > 512 {
            cache.clear()
        }
        cache.insert(text.to_owned(), ops);
    }
    let mut stack = [0.; 64];
    let mut n = 0;
    for op in &cache[text] {
        match op {
            Op::Number(v) => {
                if n == 64 {
                    return Err("Expressão complexa demais".into());
                }
                stack[n] = *v;
                n += 1;
            }
            Op::Variable(name) => {
                if n == 64 {
                    return Err("Expressão complexa demais".into());
                }
                stack[n] = vars
                    .iter()
                    .find(|(k, _)| *k == name)
                    .ok_or("Variável ausente")?
                    .1;
                n += 1;
            }
            Op::Neg => {
                if n < 1 {
                    return Err("Operação inválida".into());
                }
                stack[n - 1] = -stack[n - 1];
            }
            _ => {
                if n < 2 {
                    return Err("Operação inválida".into());
                }
                let b = stack[n - 1];
                n -= 1;
                let a = stack[n - 1];
                stack[n - 1] = match op {
                    Op::Add => a + b,
                    Op::Sub => a - b,
                    Op::Mul => a * b,
                    Op::Div => a / b,
                    _ => unreachable!(),
                };
            }
        }
    }
    if n != 1 || !stack[0].is_finite() {
        return Err("Posição inválida ou divisão por zero".into());
    }
    Ok(stack[0])
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Geometry {
    pub source: Value,
    pub baseline: Placement,
    pub scaled_at: f64,
}
impl Geometry {
    pub fn placement(&self, current: Placement, aspect: f32, scale: f32) -> Option<Placement> {
        let b = self.baseline;
        if current.center != b.center
            || current.size != b.size
            || current.width_ratio != b.width_ratio
            || current.height_ratio != b.height_ratio
        {
            return None;
        }
        let height = canvas_height();
        let r = rectangle(
            &self.source,
            aspect as f64 * height,
            height,
            scale as f64 * 100.,
        )
        .ok()?;
        let mut p = current;
        p.center = [
            ((r[0] + r[2] / 2.) / (aspect as f64 * height)) as f32,
            ((r[1] + r[3] / 2.) / height) as f32,
        ];
        p.size = (r[3] / height) as f32;
        p.width_ratio = (r[2] / r[3]) as f32;
        p.height_ratio = 1.;
        Some(p)
    }
}
fn rectangle(b: &Value, sw: f64, sh: f64, preferred: f64) -> Result<[f64; 4], String> {
    let number = |key: &str, d: f64| b[key].as_f64().unwrap_or(d);
    let (w, h) = (
        number("width", 50.) * preferred / 100.,
        number("height", 50.) * preferred / 100.,
    );
    if !w.is_finite() || !h.is_finite() || w <= 0. || h <= 0. || w > 10000. || h > 10000. {
        return Err("Dimensões inválidas".into());
    }
    let vars = [
        ("margin", 2.),
        ("width", w),
        ("height", h),
        ("screen_width", sw),
        ("screen_height", sh),
        ("preferred_scale", preferred),
        ("right", sw - w),
        ("bottom", sh - h),
    ];
    let x = expression(b["dynamicX"].as_str().unwrap_or("0"), &vars)?;
    let y = expression(b["dynamicY"].as_str().unwrap_or("0"), &vars)?;
    Ok([x, y, w, h])
}
pub fn import(value: &Value) -> Result<Settings, String> {
    let value = if let Some(s) = value.as_str() {
        serde_json::from_str(s).map_err(|e| format!("JSON inválido: {e}"))?
    } else {
        value.clone()
    };
    let version = value["version"].as_u64().unwrap_or(9);
    if version > 9 {
        return Err(format!("Layout versão {version} não suportado"));
    }
    let mut buttons = value["mControlDataList"]
        .as_array()
        .ok_or("Falta mControlDataList")?
        .clone();
    let drawers = value["mDrawerDataList"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let mut membership = BTreeMap::new();
    if drawers.len() > 64 {
        return Err("Layout excede 64 gavetas".into());
    }
    for (di, drawer) in drawers.iter().enumerate() {
        let id = (di + 1) as u64;
        let mut props = drawer["properties"].clone();
        if !props.is_object() {
            return Err("Gaveta sem properties".into());
        }
        props["keycodes"] = json!([0, 0, 0, 0]);
        membership.insert(buttons.len(), (Some(id), None));
        buttons.push(props.clone());
        let children = drawer["buttonProperties"]
            .as_array()
            .ok_or("Gaveta sem buttonProperties")?;
        let direction = drawer["orientation"].as_str().unwrap_or("FREE");
        if !["FREE", "LEFT", "RIGHT", "UP", "DOWN"].contains(&direction) {
            return Err("Orientação de gaveta inválida".into());
        }
        let mut offset = match direction {
            "RIGHT" => props["width"].as_f64().unwrap_or(50.) + 2.,
            "DOWN" => props["height"].as_f64().unwrap_or(50.) + 2.,
            _ => 0.,
        };
        for child in children {
            let mut child = child.clone();
            if direction != "FREE" {
                let horizontal = matches!(direction, "LEFT" | "RIGHT");
                let size = child[if horizontal { "width" } else { "height" }]
                    .as_f64()
                    .unwrap_or(50.);
                if matches!(direction, "LEFT" | "UP") {
                    offset -= size + 2.;
                }
                let axis = if horizontal { "dynamicX" } else { "dynamicY" };
                let other = if horizontal { "dynamicY" } else { "dynamicX" };
                child[axis] = json!(format!(
                    "({}) + px({offset}) * ${{preferred_scale}} / 100",
                    props[axis].as_str().unwrap_or("0")
                ));
                child[other] = props[other].clone();
                if matches!(direction, "RIGHT" | "DOWN") {
                    offset += size + 2.;
                }
            }
            membership.insert(buttons.len(), (None, Some(id)));
            buttons.push(child);
        }
    }
    if buttons.len() > 256 {
        return Err("Layout excede 256 botões".into());
    }
    let scaled = value["scaledAt"].as_f64().unwrap_or(100.);
    if !scaled.is_finite() || scaled <= 0. {
        return Err("scaledAt inválido".into());
    }
    // v8 uses GLFW. Keep compatibility with early Android-keycode v9 exchanges.
    let glfw = version == 8
        || buttons
            .iter()
            .flat_map(|b| b["keycodes"].as_array().into_iter().flatten())
            .any(|v| v.as_i64().is_some_and(|n| n >= 256));
    let mut settings = Settings::default_pojav();
    settings.custom_buttons.clear();
    settings.pojav_info = value.get("mControlInfoDataList").cloned();
    settings.pojav_drawers = drawers;
    if let Some(info) = settings.pojav_info.as_ref() {
        if let Some(name) = info["name"].as_str() {
            if !["default", "layout de controle padrão", "null"]
                .contains(&name.to_lowercase().as_str())
            {
                settings.preset_name = name.to_owned();
            }
        }
        if let Some(desc) = info["desc"].as_str() {
            settings.preset_description = desc.to_owned();
        }
    }
    for (i, b) in buttons.iter().enumerate() {
        let r = rectangle(b, 854., 480., 100.).map_err(|e| format!("Botão {}: {e}", i + 1))?;
        let mut button = CustomButton::new("key.keyboard.unknown".into(), i);
        let codes = b["keycodes"].as_array().ok_or("Falta keycodes")?;
        if codes.len() > 4 {
            return Err("No máximo quatro comandos por botão".into());
        }
        for (slot, code) in codes.iter().enumerate() {
            let code = code.as_i64().ok_or("Código de tecla inválido")?;
            button.set_event_slot(
                slot,
                token(code, glfw)
                    .ok_or_else(|| format!("Botão {}: código {code} não suportado", i + 1))?,
            );
        }
        button.custom_label = b["name"].as_str().unwrap_or("").to_owned();
        let mut p = Placement::new(
            ((r[0] + r[2] / 2.) / 854.) as f32,
            ((r[1] + r[3] / 2.) / 480.) as f32,
            (r[3] / 480.) as f32,
        );
        let num = |key: &str, d: f64| b[key].as_f64().unwrap_or(d);
        p.pojav_imported = true;
        p.width_ratio = (r[2] / r[3]) as f32;
        p.use_goy_style = true;
        p.shape = 0;
        p.button_color = num("bgColor", 0.) as i64 as u32;
        p.stroke_color = num("strokeColor", -1.) as i64 as u32;
        p.opacity = num("opacity", 1.) as f32;
        p.roundness = num("cornerRadius", 0.) as f32;
        p.stroke_roundness = p.roundness;
        p.stroke_size = num("strokeWidth", 0.) as f32;
        p.toggleable = b["isToggle"].as_bool().unwrap_or(false);
        p.swipeable = b["isSwipeable"].as_bool().unwrap_or(false);
        p.passthrough = b["passThruEnabled"].as_bool().unwrap_or(false);
        p.visible_in_game = b["displayInGame"].as_bool().unwrap_or(true);
        p.visible_in_menu = b["displayInMenu"].as_bool().unwrap_or(true);
        p.sanitize();
        button.layouts = [p; 3];
        button.pojav_geometry = Some(Geometry {
            source: b.clone(),
            baseline: p,
            scaled_at: scaled,
        });
        if let Some(&(drawer, parent)) = membership.get(&i) {
            button.drawer_id = drawer;
            button.parent_drawer = parent;
        }
        settings.custom_buttons.push(button);
    }
    if let Some(joysticks) = value["mJoystickDataList"].as_array() {
        if joysticks.len() > 1 {
            return Err("O motor suporta um analógico de movimento por layout".into());
        }
        if let Some(source) = joysticks.first() {
            if source["forwardLock"].as_bool() == Some(true) {
                return Err(
                    "forwardLock de analógico ainda não é suportado; layout preservado".into(),
                );
            }
            let mut record = source.clone();
            record["keycodes"] = json!([0, 0, 0, 0]);
            let one = import(&json!({"version":8,"scaledAt":scaled,"mControlDataList":[record]}))?;
            let button = &one.custom_buttons[0];
            settings.pojav_joystick = button.pojav_geometry.clone();
            for row in &mut settings.layouts {
                row[super::Control::Move as usize] = button.layouts[0];
            }
            settings.button_visibility[super::Control::Move as usize] = 0;
            settings.dynamic_joystick = !source["absolute"].as_bool().unwrap_or(false);
        }
    }
    Ok(settings)
}
pub fn export(settings: &Settings) -> Result<Value, String> {
    let encode = |b: &CustomButton| -> Result<Value, String> {
        let p = b.layouts[settings.mode.index()];
        let r = p.rect(854. / 480.);
        let mut keys = Vec::new();
        for slot in 0..4 {
            let t = b.event_slot(slot);
            let t = if t == "key.keyboard.unknown" { "" } else { t };
            let code = (-9..=348)
                .find(|c| token(*c, true).as_deref() == Some(t))
                .ok_or_else(|| format!("Comando sem equivalente Pojav: {t}"))?;
            keys.push(code);
        }
        let mut v = b
            .pojav_geometry
            .as_ref()
            .map(|g| g.source.clone())
            .unwrap_or(json!({}));
        if b.pojav_geometry
            .as_ref()
            .and_then(|g| g.placement(p, 854. / 480., 1.))
            .is_none()
        {
            v["dynamicX"] = json!(format!("${{screen_width}} * {}", r[0]));
            v["dynamicY"] = json!(format!("${{screen_height}} * {}", r[1]));
            v["width"] = json!(r[2] * 854.);
            v["height"] = json!(r[3] * 480.);
        }
        for (key, val) in [
            ("name", json!(b.custom_label)),
            ("keycodes", json!(keys)),
            ("bgColor", json!(p.button_color as i32)),
            ("strokeColor", json!(p.stroke_color as i32)),
            ("strokeWidth", json!(p.stroke_size)),
            ("cornerRadius", json!(p.roundness)),
            ("opacity", json!(p.opacity)),
            ("isToggle", json!(p.toggleable)),
            ("isSwipeable", json!(p.swipeable)),
            ("passThruEnabled", json!(p.passthrough)),
            ("displayInGame", json!(p.visible_in_game)),
            ("displayInMenu", json!(p.visible_in_menu)),
        ] {
            v[key] = val;
        }
        Ok(v)
    };
    let mut buttons = Vec::new();
    let mut drawers = Vec::new();
    let mut joysticks = Vec::new();
    for b in &settings.custom_buttons {
        if let Some(id) = b.drawer_id {
            let mut drawer = settings
                .pojav_drawers
                .get(id.saturating_sub(1) as usize)
                .cloned()
                .unwrap_or(json!({"orientation":"FREE"}));
            drawer["properties"] = encode(b)?;
            let mut children = Vec::new();
            for c in settings
                .custom_buttons
                .iter()
                .filter(|c| c.parent_drawer == Some(id))
            {
                children.push(encode(c)?);
            }
            drawer["buttonProperties"] = json!(children);
            drawers.push(drawer);
        } else if b.parent_drawer.is_none() {
            buttons.push(encode(b)?);
        }
    }
    if let Some(g) = &settings.pojav_joystick {
        let mut button = CustomButton::new("key.keyboard.unknown".into(), 0);
        button.pojav_geometry = Some(g.clone());
        button.layouts =
            std::array::from_fn(|i| settings.layouts[i][super::Control::Move as usize]);
        let mut source = encode(&button)?;
        source["absolute"] = json!(!settings.dynamic_joystick);
        source["forwardLock"] = json!(false);
        joysticks.push(source);
    }
    Ok(
        json!({"version":8,"scaledAt":settings.custom_buttons.first().and_then(|b|b.pojav_geometry.as_ref()).map_or(100.,|g|g.scaled_at),"mControlInfoDataList":settings.pojav_info,"mControlDataList":buttons,"mDrawerDataList":drawers,"mJoystickDataList":joysticks}),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn arithmetic_is_safe_and_bounded() {
        assert_eq!(
            expression(
                "px(10) * ${preferred_scale} / 100 + (2-1)",
                &[("preferred_scale", 100.)]
            )
            .unwrap(),
            11.
        );
        for x in ["${unknown}", "1/0", "px(__import__(1))"] {
            assert!(expression(x, &[]).is_err())
        }
        assert!(expression(&format!("{}1{}", "(".repeat(100), ")".repeat(100)), &[]).is_err());
    }
    #[test]
    fn glfw_commands_are_not_android_codes() {
        assert_eq!(token(87, true).as_deref(), Some("key.keyboard.w"));
        assert_eq!(token(32, true).as_deref(), Some("key.keyboard.space"));
        assert_eq!(token(340, true).as_deref(), Some("key.keyboard.left.shift"));
        assert_eq!(token(-9, true).as_deref(), Some("launcher.menu"));
        assert_eq!(token(70, true).as_deref(), Some("key.keyboard.f"));
    }
}

static CANVAS_HEIGHT: std::sync::atomic::AtomicU32 =
    std::sync::atomic::AtomicU32::new(480.0f32.to_bits());
pub fn set_canvas_height(height: f32) {
    if height.is_finite() && (100.0..=3000.0).contains(&height) {
        CANVAS_HEIGHT.store(height.to_bits(), std::sync::atomic::Ordering::Relaxed);
    }
}
fn canvas_height() -> f64 {
    f32::from_bits(CANVAS_HEIGHT.load(std::sync::atomic::Ordering::Relaxed)) as f64
}

/// Resolve imported geometry only when the player edits its dimensions/position.
/// Renaming and color changes retain the original dynamic expressions.
pub fn materialize(settings: &mut Settings, index: usize, aspect: f32) {
    let scale = settings.button_scale;
    if let Some(button) = settings.custom_buttons.get_mut(index) {
        if let Some(g) = button.pojav_geometry.clone() {
            for p in &mut button.layouts {
                if let Some(mut resolved) = g.placement(*p, aspect, scale) {
                    resolved.size /= scale.max(0.01);
                    *p = resolved;
                }
            }
            button.pojav_geometry = None;
        }
    }
}
