use super::ParticleStatus;

#[test]
fn particle_status_wraps_out_of_range_ids_like_vanilla() {
    assert_eq!(ParticleStatus::from(0), ParticleStatus::All);
    assert_eq!(ParticleStatus::from(2), ParticleStatus::Minimal);
    assert_eq!(ParticleStatus::from(3), ParticleStatus::All);
    assert_eq!(ParticleStatus::from(4), ParticleStatus::Decreased);
    assert_eq!(ParticleStatus::from(-1), ParticleStatus::Minimal);
    assert_eq!(ParticleStatus::from(-2), ParticleStatus::Decreased);
}
