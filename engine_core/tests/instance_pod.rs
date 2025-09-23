use bytemuck::{Pod, Zeroable};
use engine_core::actors::InstanceGpu;

#[test]
fn instance_gpu_pod_and_size() {
    // Compile-time trait checks are enforced by implementing Pod/Zeroable on the type
    fn _assert_pod<T: Pod + Zeroable>() {}
    _assert_pod::<InstanceGpu>();

    // Verify size is a multiple of 16 and matches expectation (4x4 f32 = 64 bytes + 16 bytes for 4 u32s = 80)
    assert_eq!(std::mem::size_of::<InstanceGpu>() % 16, 0);
    assert_eq!(std::mem::size_of::<InstanceGpu>(), 80);
}
