//! Reference launcher layout in logical pixels, backed by the existing catalog and actions.
use super::*;
use horizon_client::content_files::Kind;
use std::sync::OnceLock;
const WHITE: u32 = color::hex(0xF6F5FA, 255);
const MUTED: u32 = color::hex(0xA6A4B2, 255);
const LINE: u32 = color::hex(0x292832, 255);
const CARD: u32 = color::hex(0x111118, 255);
const ACCENT: u32 = color::hex(0x8550F6, 255);
struct Art {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}
fn art(brand: bool) -> &'static Art {
    static HERO: OnceLock<Art> = OnceLock::new();
    static BRAND: OnceLock<Art> = OnceLock::new();
    (if brand { &BRAND } else { &HERO }).get_or_init(|| {
        let bytes: &[u8] = if brand {
            include_bytes!("../../../assets/launcher/brand.png")
        } else {
            include_bytes!("../../../assets/launcher/hero.png")
        };
        let decoded = image::load_from_memory(bytes).expect("bundled original launcher art");
        let pixels = decoded
            .resize_exact(
                if brand { 96 } else { 1024 },
                if brand { 96 } else { 300 },
                image::imageops::FilterType::Lanczos3,
            )
            .to_rgba8();
        Art {
            width: pixels.width(),
            height: pixels.height(),
            rgba: pixels.into_raw(),
        }
    })
}
fn picture(ui: &mut Ui, brand: bool, r: [f32; 4]) {
    let image = art(brand);
    ui.image(
        if brand {
            "pojav/reference/brand"
        } else {
            "pojav/reference/hero"
        },
        image.width,
        image.height,
        &image.rgba,
        r[0],
        r[1],
        r[2],
        r[3],
    );
}
fn rect(ui: &mut Ui, r: [f32; 4], radius: f32) {
    ui.rounded(r[0], r[1], r[2], r[3], radius, CARD);
    ui.border(r[0], r[1], r[2], r[3], radius, LINE);
}
fn purple(ui: &mut Ui, r: [f32; 4], radius: f32, active: bool) {
    let (start, end) = if active {
        (0x7742E0u32, 0x542BB7u32)
    } else {
        (0x4B2692, 0x24183D)
    };
    ui.rounded(r[0], r[1], r[2], r[3], radius, color::hex(end, 255));
    let n = 128;
    for i in 0..n {
        let t = (i as f32 + 0.5) / n as f32;
        let dx = r[2] * t;
        let edge = if dx < radius {
            radius - dx
        } else if dx > r[2] - radius {
            dx - (r[2] - radius)
        } else {
            0.0
        };
        let inset = radius - (radius * radius - edge * edge).max(0.0).sqrt();
        let rgb = |shift: u32| {
            let a = (start >> shift) & 255u32;
            let b = (end >> shift) & 255u32;
            ((a as f32 + (b as f32 - a as f32) * t).round() as u32) << shift
        };
        ui.fill(
            r[0] + r[2] * i as f32 / n as f32,
            r[1] + inset,
            r[2] / n as f32 + 0.1,
            r[3] - inset * 2.0,
            color::hex(rgb(16) | rgb(8) | rgb(0), 255),
        );
    }
    ui.border(r[0], r[1], r[2], r[3], radius, color::hex(0x9871E4, 55));
}
fn label(ui: &mut Ui, size: f32, r: [f32; 4], text: &str, primary: bool) {
    ui.centered_fit(
        Face::SemiBold,
        size,
        r,
        if primary { WHITE } else { MUTED },
        text,
    );
}
fn small_button(
    ui: &mut Ui,
    targets: &mut Vec<([f32; 4], Action)>,
    r: [f32; 4],
    text: &str,
    action: Action,
    s: f32,
    active: bool,
) {
    if active {
        purple(ui, r, 9.0 * s, true)
    } else {
        rect(ui, r, 9.0 * s)
    };
    label(ui, 15.0 * s, r, text, true);
    targets.push((r, action));
}
#[derive(Clone, Copy)]
pub(super) struct Layout {
    pub s: f32,
    pub x: f32,
    pub w: f32,
}
impl Launcher {
    pub(super) fn reference_shell(&mut self, ui: &mut Ui) -> Layout {
        let s = (ui.w / 1232.0).min(ui.h / 576.0).max(0.25);
        let sidebar = 286.0 * s;
        ui.fill(0.0, 0.0, ui.w, ui.h, color::hex(0x0B0B11, 255));
        ui.fill(sidebar, 0.0, 1.0, ui.h, color::hex(0x24232D, 160));
        picture(ui, true, [37.0 * s, 24.0 * s, 53.0 * s, 55.0 * s]);
        // Keep the full brand inside the sidebar at every UI scale.
        let brand_width = sidebar - 103.0 * s - 18.0 * s;
        let brand = "Nivora Launcher";
        let measured = ui.measure(Face::Bold, 23.0 * s, brand);
        let brand_size = 23.0 * s * (brand_width / measured.max(1.0)).min(1.0);
        ui.text_clipped(
            Face::Bold,
            brand_size,
            103.0 * s,
            29.0 * s,
            brand_width,
            WHITE,
            brand,
        );
        ui.text(
            Face::Regular,
            14.0 * s,
            103.0 * s,
            58.0 * s,
            MUTED,
            "Minecraft no Android",
        );
        let pages = [
            (Page::Play, Icon::Play, "Jogar"),
            (Page::Mods, Icon::Cube, "Mods"),
            (Page::Packs, Icon::Packs, "Resource Packs"),
            (Page::Shaders, Icon::Monitor, "Shaders"),
            (Page::Installed, Icon::Grid, "Instalados"),
            (Page::Settings, Icon::Gear, "Configurações"),
        ];
        for (i, (page, icon, name)) in pages.into_iter().enumerate() {
            let r = [22.0 * s, (116.0 + i as f32 * 58.0) * s, 250.0 * s, 56.0 * s];
            let on = self.catalog.page == page;
            if on {
                purple(ui, r, 14.0 * s, false)
            }
            ui.icon(
                icon,
                51.0 * s,
                r[1] + 16.0 * s,
                24.0 * s,
                if on { WHITE } else { color::hex(0xCBC8D4, 255) },
            );
            ui.text(
                if on { Face::SemiBold } else { Face::Regular },
                20.0 * s,
                99.0 * s,
                r[1] + 15.0 * s,
                if on { WHITE } else { color::hex(0xCAC7D1, 255) },
                name,
            );
            self.targets.push((r, Action::Page(page)));
        }
        ui.fill(40.0 * s, ui.h - 51.0 * s, 3.0 * s, 23.0 * s, ACCENT);
        ui.text(
            Face::Regular,
            14.0 * s,
            54.0 * s,
            ui.h - 47.0 * s,
            MUTED,
            "v0.1.0",
        );
        // Header actions keep actual account names and skin selection.
        let name = self
            .login
            .profile
            .as_deref()
            .unwrap_or(&self.catalog.config.name);
        let width = (ui.measure(Face::SemiBold, 17.0 * s, name) + 84.0 * s).min(270.0 * s);
        let account = [ui.w - width - 32.0 * s, 24.0 * s, width, 52.0 * s];
        ui.avatar_head_2d(account[0] + 5.0 * s, account[1] + 4.0 * s, 43.0 * s, name);
        ui.text_clipped(
            Face::SemiBold,
            17.0 * s,
            account[0] + 61.0 * s,
            account[1] + 15.0 * s,
            width - 88.0 * s,
            WHITE,
            name,
        );
        ui.icon(
            Icon::ChevronDown,
            account[0] + width - 21.0 * s,
            account[1] + 20.0 * s,
            14.0 * s,
            MUTED,
        );
        self.targets.push((account, Action::Account));
        self.targets.push((
            [
                account[0] + 5.0 * s,
                account[1] + 4.0 * s,
                43.0 * s,
                43.0 * s,
            ],
            Action::Page(Page::Skins),
        ));
        let edit = [account[0] - 42.0 * s, 30.0 * s, 32.0 * s, 38.0 * s];
        ui.icon(
            Icon::Gamepad,
            edit[0] + 5.0 * s,
            edit[1] + 8.0 * s,
            22.0 * s,
            MUTED,
        );
        self.targets.push((edit, Action::OpenControlEditor));
        self.anim.step(
            self.catalog.page,
            self.catalog.settings_category,
            0.0,
            self.chooser.is_some() || self.login.visible || self.catalog.project_modal.is_some(),
            self.catalog.sections,
        );
        Layout {
            s,
            x: 312.0 * s,
            w: ui.w - 338.0 * s,
        }
    }
    pub(super) fn reference_body(&mut self, ui: &mut Ui, l: Layout) -> bool {
        if self.worker.is_some() || self.catalog.busy() {
            return false;
        }
        match self.catalog.page {
            Page::Play => self.reference_home(ui, l),
            Page::Mods | Page::Packs | Page::Shaders => self.reference_catalog(ui, l),
            Page::Installed
                if self.catalog.installed_sub_tab == crate::catalog::InstalledSubTab::Library =>
            {
                self.reference_installed(ui, l)
            }
            Page::Settings if self.catalog.settings_category == SettingsCategory::Overview => {
                self.reference_settings(ui, l)
            }
            _ => return false,
        }
        true
    }
    fn reference_home(&mut self, ui: &mut Ui, l: Layout) {
        let Layout { s, x, w } = l;
        let hero = [x, 106.0 * s, w, 264.0 * s];
        picture(ui, false, hero);
        ui.border(x, hero[1], w, hero[3], 28.0 * s, LINE);
        ui.text(
            Face::Medium,
            17.0 * s,
            x + 34.0 * s,
            179.0 * s,
            color::hex(0xC0BCC9, 255),
            "M I N E C R A F T   J A V A",
        );
        let version = if self.catalog.config.legacy18 {
            "1.8.9"
        } else {
            "26.3"
        };
        ui.text(
            Face::Bold,
            42.0 * s,
            x + 34.0 * s,
            205.0 * s,
            WHITE,
            &format!("Minecraft {version}"),
        );
        let renderer = if self.catalog.config.renderer == crate::catalog::RendererMode::OpenGl {
            "OpenGL"
        } else {
            "Vulkan"
        };
        let fps = if self.catalog.config.max_fps_mode {
            "Limite 260 FPS"
        } else if self.catalog.config.battery_perf_mode {
            "Limite 60 FPS"
        } else {
            "Limite 120 FPS"
        };
        let mut cx = x + 34.0 * s;
        for (text, icon, action) in [
            (version, Icon::Cube, Action::GameVersion),
            (renderer, Icon::Monitor, Action::Renderer),
            (fps, Icon::Bolt, Action::ToggleMaxFpsMode),
        ] {
            let cw = (ui.measure(Face::SemiBold, 16.0 * s, text) + 52.0 * s).max(95.0 * s);
            let r = [cx, 273.0 * s, cw, 41.0 * s];
            rect(ui, r, 21.0 * s);
            ui.icon(icon, cx + 14.0 * s, r[1] + 12.0 * s, 18.0 * s, WHITE);
            ui.text(
                Face::SemiBold,
                16.0 * s,
                cx + 40.0 * s,
                r[1] + 11.0 * s,
                WHITE,
                text,
            );
            self.targets.push((r, action));
            cx += cw + 12.0 * s;
        }
        let play = [x + w - 300.0 * s, 255.0 * s, 196.0 * s, 71.0 * s];
        purple(ui, play, 19.0 * s, true);
        ui.icon(
            Icon::Play,
            play[0] + 33.0 * s,
            play[1] + 21.0 * s,
            27.0 * s,
            WHITE,
        );
        let installed = horizon_client::official_install::ready(&self.home);
        ui.text(
            Face::SemiBold,
            25.0 * s,
            play[0] + 82.0 * s,
            play[1] + 20.0 * s,
            WHITE,
            if self.launching {
                "Abrindo…"
            } else if installed {
                "Jogar"
            } else {
                "Instalar"
            },
        );
        self.targets.push((play, Action::Play));
        let choose = [play[0] + 210.0 * s, 255.0 * s, 67.0 * s, 71.0 * s];
        rect(ui, choose, 19.0 * s);
        ui.icon(
            Icon::ChevronDown,
            choose[0] + 22.0 * s,
            choose[1] + 26.0 * s,
            23.0 * s,
            WHITE,
        );
        self.targets.push((choose, Action::GameVersion));
        let notice = crate::updater::public_notice();
        if !notice.is_empty() {
            ui.text_clipped(
                Face::Medium,
                13.0 * s,
                x + 8.0 * s,
                376.0 * s,
                (w - 16.0 * s).max(1.0),
                WHITE,
                &format!("Comunicado: {notice}"),
            );
        }
        let gap = 24.0 * s;
        let cw = (w - gap * 2.0) / 3.0;
        for (i, (name, icon, page, kind)) in [
            ("Mods", Icon::Puzzle, Page::Mods, Kind::Mods),
            ("Resource Packs", Icon::Packs, Page::Packs, Kind::Packs),
            ("Shaders", Icon::Monitor, Page::Shaders, Kind::Shaders),
        ]
        .into_iter()
        .enumerate()
        {
            let r = [x + i as f32 * (cw + gap), 398.0 * s, cw, 98.0 * s];
            rect(ui, r, 23.0 * s);
            if i == 0 {
                ui.rounded(
                    r[0] + 7.0 * s,
                    r[1] + 94.0 * s,
                    r[2] - 14.0 * s,
                    3.0 * s,
                    1.5 * s,
                    ACCENT,
                );
            }
            ui.icon(icon, r[0] + 32.0 * s, r[1] + 30.0 * s, 34.0 * s, WHITE);
            ui.text_clipped(
                Face::SemiBold,
                21.0 * s,
                r[0] + 89.0 * s,
                r[1] + 25.0 * s,
                (cw - 104.0 * s).max(1.0),
                WHITE,
                name,
            );
            let count = self
                .catalog
                .local
                .iter()
                .filter(|item| item.kind == kind && item.enabled)
                .count();
            ui.text(
                Face::Regular,
                16.0 * s,
                r[0] + 89.0 * s,
                r[1] + 54.0 * s,
                MUTED,
                &format!("{count} ativos"),
            );
            self.targets.push((r, Action::Page(page)));
        }
        // Reuse existing maintenance flows, with stable touch targets on the home screen.
        let tools_y = 506.0 * s;
        let gap = 10.0 * s;
        let width = (w - gap * 2.0) / 3.0;
        for (i, (title, action)) in [
            ("Diagnóstico", Action::Logs),
            ("Ver atualizações", Action::CheckForUpdates),
            ("Exportar relatório", Action::ExportDiagnostics),
        ]
        .into_iter()
        .enumerate()
        {
            small_button(
                ui,
                &mut self.targets,
                [x + i as f32 * (width + gap), tools_y, width, 30.0 * s],
                title,
                action,
                s,
                false,
            );
        }
    }
    fn reference_catalog(&mut self, ui: &mut Ui, l: Layout) {
        let Layout { s, x, w } = l;
        let page = self.catalog.page;
        let (name, sub, icon, kind) = match page {
            Page::Mods => (
                "Mods",
                "Instale e gerencie mods compatíveis (Horizon / Modrinth).",
                Icon::Cube,
                Kind::Mods,
            ),
            Page::Packs => (
                "Resource Packs",
                "Texturas e visuais para o seu jogo.",
                Icon::Packs,
                Kind::Packs,
            ),
            _ => (
                "Shaders",
                "Iluminação, sombras e gráficos avançados.",
                Icon::Shaders,
                Kind::Shaders,
            ),
        };
        ui.icon(icon, x, 24.0 * s, 34.0 * s, WHITE);
        ui.text(
            Face::SemiBold,
            24.0 * s,
            x + 49.0 * s,
            20.0 * s,
            WHITE,
            name,
        );
        ui.text(Face::Regular, 13.0 * s, x + 49.0 * s, 55.0 * s, MUTED, sub);
        let search_y = 91.0 * s;
        let source_w = 132.0 * s;
        let search_w = w - source_w - 119.0 * s;
        let field = [x, search_y, search_w, 43.0 * s];
        rect(ui, field, 12.0 * s);
        ui.icon(
            Icon::Search,
            x + 13.0 * s,
            search_y + 12.0 * s,
            20.0 * s,
            MUTED,
        );
        let hint = match page {
            Page::Mods => "Buscar mods…",
            Page::Packs => "Buscar resource packs…",
            _ => "Buscar shaders…",
        };
        let query = if self.catalog.query.is_empty() {
            hint
        } else {
            &self.catalog.query
        };
        ui.text_clipped(
            Face::Regular,
            15.0 * s,
            x + 44.0 * s,
            search_y + 12.0 * s,
            search_w - 58.0 * s,
            if self.catalog.query.is_empty() {
                MUTED
            } else {
                WHITE
            },
            query,
        );
        self.targets.push((field, Action::Field("search")));
        let source = [x + search_w + 12.0 * s, search_y, source_w, 43.0 * s];
        rect(ui, source, 12.0 * s);
        let origin = if page == Page::Shaders {
            self.catalog.config.shader_source
        } else {
            self.catalog.config.mod_source
        };
        let source_name = if page == Page::Packs {
            "Modrinth"
        } else if origin == crate::shader_catalog::Origin::Horizon {
            "Horizon"
        } else {
            "Modrinth"
        };
        label(
            ui,
            14.0 * s,
            [source[0], source[1], source[2] - 22.0 * s, source[3]],
            source_name,
            true,
        );
        ui.icon(
            Icon::ChevronDown,
            source[0] + source_w - 24.0 * s,
            search_y + 15.0 * s,
            13.0 * s,
            MUTED,
        );
        self.targets.push((source, Action::CatalogSources));
        small_button(
            ui,
            &mut self.targets,
            [x + w - 95.0 * s, search_y, 95.0 * s, 43.0 * s],
            "Buscar",
            Action::Search,
            s,
            true,
        );
        // Real results, including built-in native mods. No decorative fake install entries.
        let mut rows: Vec<(String, String, String, Action, Action, bool)> = Vec::new();
        if page == Page::Mods {
            let q = self.catalog.query.to_lowercase();
            for (id, title, description) in horizon_client::mod_list::ANDROID_BUNDLED {
                if horizon_client::mod_list::bundled_visible(id, self.catalog.config.legacy18)
                    && (q.is_empty() || title.to_lowercase().contains(&q) || id.contains(&q)) {
                    rows.push((
                        id.into(),
                        title.into(),
                        description.into(),
                        Action::OpenBundledProject(id),
                        Action::ToggleBundled(id),
                        self.catalog.bundled_enabled.contains(id),
                    ));
                }
            }
        }
        for (i, p) in self.catalog.results.iter().enumerate() {
            if page == Page::Mods && rows.iter().any(|row| row.0 == p.id) {
                continue;
            }
            rows.push((
                p.id.clone(),
                p.title.clone(),
                p.summary.clone(),
                Action::OpenProject(i),
                if self.catalog.download_label(p).is_some() {
                    Action::OpenProject(i)
                } else {
                    Action::Install(i)
                },
                self.catalog.download_label(p).is_some(),
            ));
        }
        let local_page = page == Page::Mods
            || (page == Page::Shaders && origin == crate::shader_catalog::Origin::Horizon);
        let total_pages = if local_page {
            rows.len()
                .div_ceil(crate::catalog::CATALOG_PAGE_SIZE)
                .max(1)
        } else {
            1
        };
        if local_page {
            self.catalog.offset = self
                .catalog
                .offset
                .min((total_pages - 1) * crate::catalog::CATALOG_PAGE_SIZE);
        }
        let start = if local_page { self.catalog.offset } else { 0 };
        let rows: Vec<_> = rows
            .into_iter()
            .skip(start)
            .take(crate::catalog::CATALOG_PAGE_SIZE)
            .collect();
        let top = 150.0 * s;
        let bottom = ui.h - 68.0 * s;
        let row_h = 78.0 * s;
        let max = (rows.len() as f32 * row_h - (bottom - top)).max(0.0);
        self.catalog.scroll = self.catalog.scroll.clamp(0.0, max);
        self.list_view = Some([top, bottom, max]);
        ui.set_clip(Some([x - 1.0, top, w + 2.0, (bottom - top).max(1.0)]));
        for (i, (id, title, description, open, install, enabled)) in rows.iter().enumerate() {
            let r = [x, top + i as f32 * row_h - self.catalog.scroll, w, 67.0 * s];
            self.list_targets.push((r, *open));
            if r[1] + r[3] < top || r[1] > bottom {
                continue;
            }
            rect(ui, r, 13.0 * s);
            let ir = [x + 13.0 * s, r[1] + 11.0 * s, 45.0 * s, 45.0 * s];
            let drawn = self.catalog.icons.get(id).is_some_and(|(iw, ih, rgba)| {
                ui.image(id, *iw, *ih, rgba, ir[0], ir[1], ir[2], ir[3])
            });
            if !drawn {
                purple(ui, ir, 10.0 * s, true);
                ui.icon(
                    if page == Page::Mods {
                        Icon::Puzzle
                    } else {
                        icon
                    },
                    ir[0] + 8.0 * s,
                    ir[1] + 8.0 * s,
                    29.0 * s,
                    WHITE,
                );
            }
            ui.text_clipped(
                Face::SemiBold,
                17.0 * s,
                x + 74.0 * s,
                r[1] + 11.0 * s,
                w - 208.0 * s,
                WHITE,
                title,
            );
            ui.text_clipped(
                Face::Regular,
                13.0 * s,
                x + 74.0 * s,
                r[1] + 38.0 * s,
                w - 208.0 * s,
                MUTED,
                description,
            );
            if let Some(clip) = ui.clip_rect(r) {
                self.targets.push((clip, *open));
            }
            small_button(
                ui,
                &mut self.targets,
                [x + w - 111.0 * s, r[1] + 15.0 * s, 96.0 * s, 36.0 * s],
                if !*enabled {
                    "Instalar"
                } else if matches!(install, Action::ToggleBundled(_)) {
                    "Ativo"
                } else {
                    "Instalado"
                },
                *install,
                s,
                true,
            );
        }
        ui.set_clip(None);
        if rows.is_empty() {
            ui.text(
                Face::Regular,
                16.0 * s,
                x,
                top + 24.0 * s,
                MUTED,
                if self.catalog.search_worker_active() {
                    "Buscando conteúdo…"
                } else {
                    "Nenhum resultado. Tente outra busca ou importe um arquivo."
                },
            );
        }
        let y = ui.h - 48.0 * s;
        small_button(
            ui,
            &mut self.targets,
            [x + w * 0.5 - 80.0 * s, y, 35.0 * s, 32.0 * s],
            "‹",
            Action::Previous,
            s,
            false,
        );
        label(
            ui,
            13.0 * s,
            [x + w * 0.5 - 35.0 * s, y, 70.0 * s, 32.0 * s],
            &format!(
                "{} / {}",
                self.catalog.offset / crate::catalog::CATALOG_PAGE_SIZE + 1,
                total_pages
            ),
            true,
        );
        small_button(
            ui,
            &mut self.targets,
            [x + w * 0.5 + 45.0 * s, y, 35.0 * s, 32.0 * s],
            "›",
            Action::Next,
            s,
            false,
        );
        small_button(
            ui,
            &mut self.targets,
            [x, y, 90.0 * s, 32.0 * s],
            "Importar",
            Action::Import(kind),
            s,
            false,
        );
        self.targets.push((
            [x + w - 38.0 * s, y, 38.0 * s, 32.0 * s],
            Action::OpenFolder(kind),
        ));
        ui.icon(Icon::Folder, x + w - 30.0 * s, y + 6.0 * s, 22.0 * s, MUTED);
    }
    fn reference_installed(&mut self, ui: &mut Ui, l: Layout) {
        use crate::catalog::InstalledFilter;
        let Layout { s, x, w } = l;
        ui.icon(Icon::Grid, x, 24.0 * s, 34.0 * s, WHITE);
        ui.text(
            Face::SemiBold,
            24.0 * s,
            x + 49.0 * s,
            20.0 * s,
            WHITE,
            "Instalados",
        );
        ui.text(
            Face::Regular,
            13.0 * s,
            x + 49.0 * s,
            55.0 * s,
            MUTED,
            "Gerencie seus mods, packs e shaders.",
        );
        let gap = 10.0 * s;
        let cw = (w - 3.0 * gap) / 4.0;
        for (i, f) in InstalledFilter::ALL.into_iter().enumerate() {
            let count = self
                .catalog
                .local
                .iter()
                .filter(|item| f.matches(item.kind))
                .count();
            let name = match f {
                InstalledFilter::All => "Todos",
                InstalledFilter::Mods => "Mods",
                InstalledFilter::Packs => "Packs",
                InstalledFilter::Shaders => "Shaders",
            };
            let r = [x + i as f32 * (cw + gap), 91.0 * s, cw, 40.0 * s];
            small_button(
                ui,
                &mut self.targets,
                r,
                &format!("{name} ({count})"),
                Action::SetInstalledFilter(f),
                s,
                self.catalog.installed_filter == f,
            );
        }
        let rows: Vec<_> = self
            .catalog
            .local
            .iter()
            .cloned()
            .enumerate()
            .filter(|(_, item)| self.catalog.installed_filter.matches(item.kind))
            .collect();
        let top = 150.0 * s;
        let bottom = ui.h - 58.0 * s;
        let row_h = 86.0 * s;
        let max = (rows.len() as f32 * row_h - (bottom - top)).max(0.0);
        self.catalog.scroll = self.catalog.scroll.clamp(0.0, max);
        self.list_view = Some([top, bottom, max]);
        ui.set_clip(Some([x - 1.0, top, w + 2.0, (bottom - top).max(1.0)]));
        for (row, (i, item)) in rows.iter().enumerate() {
            let r = [
                x,
                top + row as f32 * row_h - self.catalog.scroll,
                w,
                75.0 * s,
            ];
            if r[1] + r[3] < top || r[1] > bottom {
                continue;
            }
            rect(ui, r, 14.0 * s);
            let icon = match item.kind {
                Kind::Mods => Icon::Puzzle,
                Kind::Packs => Icon::Packs,
                _ => Icon::Monitor,
            };
            let ir = [x + 14.0 * s, r[1] + 14.0 * s, 48.0 * s, 48.0 * s];
            let raw = item
                .file
                .strip_prefix("builtin:")
                .or_else(|| item.file.strip_prefix("horizon-native-mods/"))
                .or_else(|| item.file.strip_prefix("horizon-mods/"))
                .unwrap_or(&item.file);
            let stem = raw.rsplit_once('.').map_or(raw, |(stem, _)| stem);
            let existing_icon = self
                .catalog
                .icons
                .get(&item.file)
                .map(|v| (item.file.as_str(), v))
                .or_else(|| self.catalog.icons.get(stem).map(|v| (stem, v)));
            let drawn = existing_icon.is_some_and(|(id, (iw, ih, rgba))| {
                ui.image(id, *iw, *ih, rgba, ir[0], ir[1], ir[2], ir[3])
            });
            if !drawn {
                purple(ui, ir, 12.0 * s, true);
            }
            if !drawn {
                ui.icon(icon, ir[0] + 9.0 * s, ir[1] + 9.0 * s, 30.0 * s, WHITE);
            }
            let title = ui.fit_text(Face::SemiBold, 18.0 * s, &item.title, w - 265.0 * s);
            let tw = ui.text(
                Face::SemiBold,
                18.0 * s,
                x + 80.0 * s,
                r[1] + 15.0 * s,
                WHITE,
                &title,
            );
            let badge = [x + tw + 94.0 * s, r[1] + 15.0 * s, 48.0 * s, 22.0 * s];
            ui.rounded(
                badge[0],
                badge[1],
                badge[2],
                badge[3],
                6.0 * s,
                color::hex(0x30214D, 255),
            );
            label(
                ui,
                11.0 * s,
                badge,
                if item.enabled { "Ativo" } else { "Inativo" },
                true,
            );
            ui.text_clipped(
                Face::Regular,
                13.0 * s,
                x + 80.0 * s,
                r[1] + 45.0 * s,
                w - 250.0 * s,
                MUTED,
                &format!(
                    "{} · {:.1} KiB",
                    item.subtitle,
                    item.size_bytes as f64 / 1024.0
                ),
            );
            let toggle = [x + w - 125.0 * s, r[1] + 25.0 * s, 59.0 * s, 28.0 * s];
            ui.rounded(
                toggle[0],
                toggle[1],
                toggle[2],
                toggle[3],
                14.0 * s,
                if item.enabled { ACCENT } else { LINE },
            );
            ui.rounded(
                toggle[0] + if item.enabled { 34.0 * s } else { 4.0 * s },
                toggle[1] + 4.0 * s,
                20.0 * s,
                20.0 * s,
                10.0 * s,
                WHITE,
            );
            self.targets.push((toggle, Action::ToggleLocal(*i)));
            self.list_targets.push((toggle, Action::ToggleLocal(*i)));
            small_button(
                ui,
                &mut self.targets,
                [x + w - 49.0 * s, r[1] + 22.0 * s, 34.0 * s, 34.0 * s],
                "…",
                Action::LocalOptions(*i),
                s,
                false,
            );
        }
        ui.set_clip(None);
        if rows.is_empty() {
            ui.text(
                Face::Regular,
                16.0 * s,
                x,
                top + 25.0 * s,
                MUTED,
                "Nenhum conteúdo instalado nesta categoria.",
            );
        }
        small_button(
            ui,
            &mut self.targets,
            [x, ui.h - 42.0 * s, 150.0 * s, 30.0 * s],
            "Gerenciar arquivos",
            Action::SetInstalledSubTab(crate::catalog::InstalledSubTab::FileManager),
            s,
            false,
        );
    }
    fn reference_settings(&mut self, ui: &mut Ui, l: Layout) {
        let Layout { s, x, w } = l;
        ui.icon(Icon::Gear, x, 24.0 * s, 34.0 * s, WHITE);
        ui.text(
            Face::SemiBold,
            24.0 * s,
            x + 49.0 * s,
            20.0 * s,
            WHITE,
            "Configurações",
        );
        ui.text(
            Face::Regular,
            13.0 * s,
            x + 49.0 * s,
            55.0 * s,
            MUTED,
            "Ajuste desempenho, controles e aparência do launcher.",
        );
        let items = [
            (
                Icon::Monitor,
                "Vídeo",
                "Renderização, resolução e FPS",
                Action::SettingsCat(SettingsCategory::Video),
            ),
            (
                Icon::Gear,
                "Launcher",
                "Tema, perfis e comportamento",
                Action::SettingsCat(SettingsCategory::Launcher),
            ),
            (
                Icon::Gamepad,
                "Controles",
                "HUD, botões e sensibilidade",
                Action::SettingsCat(SettingsCategory::Controls),
            ),
            (
                Icon::Flask,
                "Experimental",
                "Recursos em teste (instáveis)",
                Action::SettingsCat(SettingsCategory::Experimental),
            ),
            (
                Icon::Terminal,
                "Diagnóstico",
                "Logs, informações e erros",
                Action::SettingsCat(SettingsCategory::Diagnostics),
            ),
            (
                Icon::MicrosoftGrid,
                "Conta",
                "Microsoft / Xbox (opcional)",
                Action::Account,
            ),
        ];
        let cw = (w - 18.0 * s) / 2.0;
        for (i, (icon, title, description, action)) in items.into_iter().enumerate() {
            let r = [
                x + (i % 2) as f32 * (cw + 18.0 * s),
                (110.0 + (i / 2) as f32 * 106.0) * s,
                cw,
                88.0 * s,
            ];
            rect(ui, r, 16.0 * s);
            ui.icon(icon, r[0] + 23.0 * s, r[1] + 27.0 * s, 30.0 * s, WHITE);
            ui.text(
                Face::SemiBold,
                18.0 * s,
                r[0] + 76.0 * s,
                r[1] + 20.0 * s,
                WHITE,
                title,
            );
            ui.text_clipped(
                Face::Regular,
                12.0 * s,
                r[0] + 76.0 * s,
                r[1] + 49.0 * s,
                cw - 111.0 * s,
                MUTED,
                description,
            );
            ui.icon(
                Icon::ArrowRight,
                r[0] + cw - 30.0 * s,
                r[1] + 34.0 * s,
                16.0 * s,
                MUTED,
            );
            self.targets.push((r, action));
        }
    }
}
