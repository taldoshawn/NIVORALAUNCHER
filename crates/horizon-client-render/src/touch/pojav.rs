//! PojavLauncher v9 button exchange. Coordinates use a reference landscape canvas.
use super::{CustomButton, Settings, Placement};
use serde_json::{Value, json};
fn token(code: i64) -> Option<String> {
    Some(match code {
        0 => return Some(String::new()), -1 => "keyboard.show".into(), -2 => "key.keyboard.f1".into(),
        -3 => "key.mouse.left".into(), -4 => "key.mouse.right".into(), -5 => "mouse.virtual.toggle".into(),
        29..=54 => format!("key.keyboard.{}", char::from(b'a' + (code-29) as u8)),
        7..=16 => format!("key.keyboard.{}", code-7),
        131..=142 => format!("key.keyboard.f{}", code-130),
        59 => "key.keyboard.left.shift".into(), 60 => "key.keyboard.right.shift".into(),
        61 => "key.keyboard.tab".into(), 62 => "key.keyboard.space".into(),
        66 => "key.keyboard.enter".into(), 67 => "key.keyboard.backspace".into(),
        111 => "key.keyboard.escape".into(), 113 => "key.keyboard.left.control".into(),
        _ => return None,
    })
}
fn expression(text: &str, vars: &[(&str, f64)]) -> Result<f64,String> {
    let mut text = text.to_owned();
    for (key,value) in vars { text = text.replace(&format!("${{{key}}}"), &value.to_string()); }
    // Recursive descent, never executes imported text.
    fn sum(s: &[u8], p: &mut usize) -> Result<f64,String> {
        let mut v=product(s,p)?;
        loop { space(s,p); match s.get(*p) { Some(b'+')=>{*p+=1;v+=product(s,p)?},Some(b'-')=>{*p+=1;v-=product(s,p)?},_=>break } } Ok(v)
    }
    fn space(s:&[u8],p:&mut usize){while s.get(*p).is_some_and(u8::is_ascii_whitespace){*p+=1;}}
    fn product(s:&[u8],p:&mut usize)->Result<f64,String>{let mut v=atom(s,p)?;loop{space(s,p);match s.get(*p){Some(b'*')=>{*p+=1;v*=atom(s,p)?},Some(b'/')=>{*p+=1;v/=atom(s,p)?},_=>break}}Ok(v)}
    fn atom(s:&[u8],p:&mut usize)->Result<f64,String>{space(s,p);if s.get(*p)==Some(&b'-'){*p+=1;return Ok(-atom(s,p)?)}if s.get(*p)==Some(&b'('){*p+=1;let v=sum(s,p)?;space(s,p);if s.get(*p)!=Some(&b')'){return Err("Missing ')'".into())}*p+=1;return Ok(v)}let start=*p;while s.get(*p).is_some_and(|c|c.is_ascii_digit()||*c==b'.'){*p+=1;}std::str::from_utf8(&s[start..*p]).ok().and_then(|s|s.parse().ok()).ok_or_else(||"Unsupported coordinate expression".into())}
    if text.len()>4096 {return Err("Coordinate expression too long".into())}
    let mut pos=0;let v=sum(text.as_bytes(),&mut pos)?;space(text.as_bytes(),&mut pos);
    if pos!=text.len()||!v.is_finite(){return Err("Invalid coordinate expression".into())}Ok(v)
}
pub fn import(value:&Value)->Result<Settings,String>{
    let value=if let Some(s)=value.as_str(){serde_json::from_str(s).map_err(|e|format!("Invalid embedded JSON: {e}"))?}else{value.clone()};
    for key in ["mDrawerDataList","mJoystickDataList"] {if value[key].as_array().is_some_and(|a|!a.is_empty()){return Err(format!("{key} is not supported yet; layout was not changed"))}}
    let buttons=value["mControlDataList"].as_array().ok_or("Missing mControlDataList")?;
    if buttons.len()>256{return Err("Too many buttons".into())}
    let mut settings=Settings::default_pojav();settings.custom_buttons.clear();
    for (i,b) in buttons.iter().enumerate(){
        let num=|k:&str,d:f64|b[k].as_f64().unwrap_or(d);
        let (w,h)=(num("width",50.),num("height",50.));if w<=0.||h<=0.{return Err("Invalid button dimensions".into())}
        let vars=[("margin",2.),("width",w),("height",h),("screen_width",854.),("screen_height",480.),("right",854.-w),("bottom",480.-h)];
        let coordinate=|k:&str|expression(b[k].as_str().unwrap_or("0"),&vars);
        let (x,y)=(coordinate("dynamicX")?,coordinate("dynamicY")?);
        let mut button=CustomButton::new("key.keyboard.unknown".into(),i);
        for (slot,code) in b["keycodes"].as_array().ok_or("Missing keycodes")?.iter().take(4).enumerate(){let code=code.as_i64().ok_or("Invalid keycode")?;let t=token(code).ok_or_else(||format!("Unsupported Pojav keycode {code}"))?;button.set_event_slot(slot,t);}
        button.custom_label=b["name"].as_str().unwrap_or("").into();
        let mut p=Placement::new(((x+w/2.)/854.) as f32,((y+h/2.)/480.) as f32,(h/480.) as f32);
        p.width_ratio=(w/h) as f32;p.use_goy_style=true;p.shape=1;
        p.button_color=num("bgColor",0.) as i64 as u32;p.stroke_color=num("strokeColor",-1.) as i64 as u32;
        p.opacity=num("opacity",1.) as f32;p.roundness=num("cornerRadius",0.) as f32;p.stroke_size=num("strokeWidth",0.) as f32;
        p.toggleable=b["isToggle"].as_bool().unwrap_or(false);p.swipeable=b["isSwipeable"].as_bool().unwrap_or(false);p.passthrough=b["passThruEnabled"].as_bool().unwrap_or(false);
        p.visible_in_game=b["displayInGame"].as_bool().unwrap_or(true);p.visible_in_menu=b["displayInMenu"].as_bool().unwrap_or(true);button.layouts=[p;3];settings.custom_buttons.push(button);
    }Ok(settings)
}
pub fn export(settings:&Settings)->Result<Value,String>{
    let mut buttons=Vec::new();for b in &settings.custom_buttons {
        if b.drawer_id.is_some()||b.parent_drawer.is_some(){return Err("Drawer export is not supported".into())}
        let p=b.layouts[settings.mode.index()];let r=p.rect(854./480.);let mut keys=Vec::new();
        for slot in 0..4 {let t=b.event_slot(slot);let t=if t=="key.keyboard.unknown" { "" } else { t };let code=(-5..=142).find(|c|token(*c).as_deref()==Some(t)).ok_or_else(||format!("No Pojav keycode for {t}"))?;keys.push(code);}
        buttons.push(json!({"name":b.custom_label,"width":r[2]*854.,"height":r[3]*480.,"dynamicX":format!("${{screen_width}} * {}",r[0]),"dynamicY":format!("${{screen_height}} * {}",r[1]),"keycodes":keys,"bgColor":p.button_color as i32,"strokeColor":p.stroke_color as i32,"strokeWidth":p.stroke_size,"cornerRadius":p.roundness,"opacity":p.opacity,"isToggle":p.toggleable,"isSwipeable":p.swipeable,"passThruEnabled":p.passthrough,"displayInGame":p.visible_in_game,"displayInMenu":p.visible_in_menu}));
    }Ok(json!({"version":9,"scaledAt":100.,"mControlDataList":buttons,"mDrawerDataList":[],"mJoystickDataList":[]}))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn imports_dynamic_coordinates_and_roundtrips_button_properties() {
        let value=json!({"version":9,"mControlDataList":[{"name":"Meu botão","width":50.,"height":50.,"dynamicX":"${margin} * 2 + ${width}","dynamicY":"${bottom} - ${margin}","keycodes":[51,29,0,0],"bgColor":1291845632i64,"strokeColor":-1,"isToggle":true,"isSwipeable":true,"opacity":0.5}],"mDrawerDataList":[],"mJoystickDataList":[]});
        let settings=import(&value).unwrap();let b=&settings.custom_buttons[0];assert_eq!(b.key,"key.keyboard.w");assert_eq!(b.second_key,"key.keyboard.a");assert_eq!(b.custom_label,"Meu botão");
        let rect=b.layouts[0].rect(854./480.);assert!((rect[0]*854.-54.).abs()<0.01);assert!((rect[1]*480.-428.).abs()<0.01);
        let restored=import(&export(&settings).unwrap()).unwrap();let r=&restored.custom_buttons[0];assert_eq!(r.custom_label,b.custom_label);assert_eq!(r.key,b.key);assert_eq!(r.layouts[0].button_color,1291845632);assert!(r.layouts[0].toggleable && r.layouts[0].swipeable);
    }
    #[test]
    fn rejects_unknown_inputs_instead_of_replacing_layout_with_defaults() {
        assert!(import(&json!({"mControlDataList":[{"keycodes":[999],"dynamicX":"0","dynamicY":"0"}]})).is_err());
        assert!(expression("${unknown}",&[]).is_err());assert!(expression("1/0",&[]).is_err());
        assert_eq!(expression("(2 + 3) * 4 - 1",&[]).unwrap(),19.);
    }
}
