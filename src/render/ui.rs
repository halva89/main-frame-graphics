// Геометрия собственной панели окна (замена системного декоратора).
// Все координаты — в NDC, как их ожидает оверлейный пасс (орто-проекция -1..1).
// В Vulkan NDC ось Y направлена вниз, поэтому -1 — верх экрана, +1 — низ.

#[derive(Debug, Clone, Copy)]
pub struct UiLayout {
    pub panel_left: f32,
    pub panel_right: f32,
    pub panel_bottom: f32,
    pub panel_top: f32,
    pub corner_x: f32,
    pub corner_y: f32,
    pub btn_left: f32,
    pub btn_right: f32,
    pub btn_radius_x: f32,
    pub btn_radius_y: f32,
    pub close_bottom: f32,
    pub close_top: f32,
    pub minimize_bottom: f32,
    pub minimize_top: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiHit {
    None,
    Panel,
    Close,
    Minimize,
}

pub fn layout(width: f32, height: f32) -> UiLayout {
    let w = width.max(1.0);
    let h = height.max(1.0);
    let gap_r = 20.0 / w;
    let gap_v = 20.0 / h;
    let pw = 80.0 / w;
    let panel_right = 1.0 - gap_r;
    let panel_left = panel_right - pw;
    let panel_bottom = -1.0 + gap_v;
    let panel_top = 1.0 - gap_v;
    let btn_left = panel_left + 12.0 / w;
    let btn_sz_x = 56.0 / w;
    let btn_sz_y = 56.0 / h;
    let btn_right = btn_left + btn_sz_x;
    let btn_pad_b = 10.0 / h;
    let gap_b = 10.0 / h;
    let close_bottom = panel_bottom + btn_pad_b;
    let close_top = close_bottom + btn_sz_y;
    let minimize_bottom = close_top + gap_b;
    let minimize_top = minimize_bottom + btn_sz_y;

    UiLayout {
        panel_left,
        panel_right,
        panel_bottom,
        panel_top,
        corner_x: 30.0 / w,
        corner_y: 30.0 / h,
        btn_left,
        btn_right,
        btn_radius_x: 20.0 / w,
        btn_radius_y: 20.0 / h,
        close_bottom,
        close_top,
        minimize_bottom,
        minimize_top,
    }
}

pub fn mouse_to_ndc(width: f32, height: f32, mx: f32, my: f32) -> (f32, f32) {
    let w = width.max(1.0);
    let h = height.max(1.0);
    (mx / w * 2.0 - 1.0, my / h * 2.0 - 1.0)
}

pub fn hit_test(l: &UiLayout, x: f32, y: f32) -> UiHit {
    let in_x = x >= l.btn_left && x <= l.btn_right;
    if in_x && y >= l.close_bottom && y <= l.close_top {
        return UiHit::Close;
    }
    if in_x && y >= l.minimize_bottom && y <= l.minimize_top {
        return UiHit::Minimize;
    }
    if x >= l.panel_left && x <= l.panel_right && y >= l.panel_bottom && y <= l.panel_top {
        return UiHit::Panel;
    }
    UiHit::None
}
