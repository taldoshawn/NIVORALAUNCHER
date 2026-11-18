use horizon_client::{legacy_rules::*, sprint::Keys};
use horizon_core::geometry::Vec3;

#[test]
fn legacy_jump_sets_the_documented_impulse_and_boost() {
    assert_eq!(jump_velocity(None), 0.41999998688697815);
    assert_eq!(jump_velocity(Some(0)), 0.5199999883770943);
}

#[test]
fn tiny_vertical_velocity_is_zeroed_at_the_legacy_apex_threshold() {
    let v = trim_velocity(Vec3::new(0.004, -0.004, 0.005));
    assert_eq!((v.x,v.y,v.z), (0.0,0.0,0.005));
    // A normal falling impulse is not discarded.
    assert_eq!(trim_velocity(Vec3::new(0.0,-0.0784,0.0)).y,-0.0784);
}

#[test]
fn diagonal_sneak_and_item_input_keep_legacy_axis_scaling() {
    let keys = Keys { forward: true, left: true, ..Default::default() };
    assert_eq!(movement_input(keys,false),[0.98,0.98]);
    assert_eq!(movement_input(Keys { backward: true, ..keys },false),[0.98,0.0]);
    let v=movement_input(Keys { shift:true, ..keys },true);
    assert!((v[0]-0.0588).abs()<1e-7 && (v[1]-0.0588).abs()<1e-7);
}

#[test]
fn server_fire_clears_on_respawn_reconnect_and_extinguish_but_keeps_legitimate_fire() {
    let mut fire=RemoteFire::default();
    fire.metadata(1); // lava, then health/death packets do not invent an extinguish
    assert_eq!(fire.ticks(),1);
    fire.respawn(false); // death or legacy dimension transfer
    assert_eq!(fire.ticks(),0);
    fire.metadata(1); // server legitimately ignites the new player
    assert_eq!(fire.ticks(),1);
    fire.respawn(true); // modern KEEP_ENTITY_DATA retains authoritative flag
    assert_eq!(fire.ticks(),1);
    fire.metadata(0x08); // sprint without burning
    assert_eq!(fire.ticks(),0);
    fire.metadata(1);
    fire=RemoteFire::default(); // new connection must not inherit old metadata
    assert_eq!(fire.ticks(),0);
}
