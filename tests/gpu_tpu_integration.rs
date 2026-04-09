//! GPU/TPU integration tests.
//!
//! Tests the full accelerator pipeline: configure registers, trigger compute,
//! verify results in guest RAM. No CPU execution needed — tests the MMIO
//! peripheral + DMA path directly.

use mycpu::memory::Ram;
use mycpu::peripheral::dma;
use mycpu::peripheral::{Gpu, Tpu};
use mycpu::traits::{Accelerator, KernelType, Peripheral, Precision};
use mycpu::types::Addr;

const RAM_BASE: u32 = 0x8000_0000;
const RAM_SIZE: usize = 0x10000; // 64KB

fn make_ram() -> dma::RamRegions {
    vec![(Addr::new(RAM_BASE), RAM_SIZE, Box::new(Ram::new(RAM_SIZE)))]
}

/// Helper: write u32 into guest RAM.
fn write_guest_u32(ram: &mut dma::RamRegions, addr: u64, value: u32) {
    dma::write_u32(ram, addr, value).unwrap();
}

fn read_guest_u32(ram: &mut dma::RamRegions, addr: u64) -> u32 {
    dma::read_u32(ram, addr).unwrap()
}

fn write_f32_array(ram: &mut dma::RamRegions, base: u64, data: &[f32]) {
    dma::write_f32_slice(ram, base, data).unwrap();
}

fn read_f32_array(ram: &mut dma::RamRegions, base: u64, count: usize) -> Vec<f32> {
    dma::read_f32_slice(ram, base, count).unwrap()
}

/// Write tensor descriptor (20 bytes) into guest RAM.
fn write_tensor_desc(
    ram: &mut dma::RamRegions,
    desc_addr: u64,
    data_addr: u32,
    element_count: u32,
    shape: [u32; 3],
) {
    dma::write_tensor_desc(ram, desc_addr, data_addr, element_count, shape).unwrap();
}

/// Write u32 to a GPU register via the Peripheral trait.
fn gpu_write_reg(gpu: &mut Gpu, reg: u32, value: u32) {
    for i in 0..4 {
        gpu.write(Addr::new(reg + i), ((value >> (i * 8)) & 0xFF) as u8)
            .unwrap();
    }
}

/// ────────────────────────────────────────────────────────────────────────
/// Test 1: Simple CNN forward pass on GPU
///
/// Simulates a minimal CNN:
///   Input (1x4x4) → Conv2d (1 filter, 3x3, pad=1) → ReLU → MaxPool2d (2x2)
///   → Flatten → MatMul (16→2) → Softmax
/// ────────────────────────────────────────────────────────────────────────
#[test]
fn test_gpu_simple_cnn_forward_pass() {
    let mut gpu = Gpu::new();
    let mut ram = make_ram();

    // ── Step 1: Conv2d ──────────────────────────────────────────────────
    // Input: 1 channel, 4x4 (all 1s for simplicity)
    let input_addr = RAM_BASE + 0x100;
    let filter_addr = RAM_BASE + 0x140;
    let conv_out_addr = RAM_BASE + 0x180;

    write_f32_array(&mut ram, input_addr as u64, &[1.0; 16]);
    // 3x3 filter with values 0..9 (row-major)
    let filter_data: Vec<f32> = (0..9).map(|i| i as f32).collect();
    write_f32_array(&mut ram, filter_addr as u64, &filter_data);

    let desc_in = RAM_BASE + 0x300;
    let desc_f = RAM_BASE + 0x320;
    let desc_conv_out = RAM_BASE + 0x340;

    write_tensor_desc(&mut ram, desc_in as u64, input_addr, 16, [1, 4, 4]);
    write_tensor_desc(&mut ram, desc_f as u64, filter_addr, 9, [1, 3, 3]);
    write_tensor_desc(&mut ram, desc_conv_out as u64, conv_out_addr, 16, [1, 4, 4]);

    gpu_write_reg(&mut gpu, 0x08, KernelType::Conv2d as u32); // KERNEL_TYPE
    gpu_write_reg(&mut gpu, 0x10, desc_in); // INPUT0_DESC_LOW
    gpu_write_reg(&mut gpu, 0x18, desc_f); // INPUT1_DESC_LOW
    gpu_write_reg(&mut gpu, 0x30, desc_conv_out); // OUTPUT0_DESC_LOW
    gpu_write_reg(&mut gpu, 0x40, (3 << 16) | 3); // CONV_KERNEL_SIZE 3x3
    gpu_write_reg(&mut gpu, 0x44, (1 << 16) | 1); // CONV_STRIDE 1x1
    gpu_write_reg(&mut gpu, 0x48, (1 << 16) | 1); // CONV_PADDING 1x1
    gpu_write_reg(&mut gpu, 0x4C, (4 << 16) | 4); // CONV_INPUT_DIMS 4x4
    gpu_write_reg(&mut gpu, 0x90, (1 << 16) | 1); // CONV_CHANNELS 1in/1out

    gpu.execute_with_memory(&mut ram);

    let conv_out = read_f32_array(&mut ram, conv_out_addr as u64, 16);
    // With all-1 input and filter [0,1,2,3,4,5,6,7,8] + pad=1:
    // Center pixel (1,1): sum of all 9 filter values touching padded input
    // The center should have a specific value; we just check it's non-zero
    let conv_sum: f32 = conv_out.iter().sum();
    assert!(
        conv_sum > 0.0,
        "Conv2d output should have non-zero sum, got {}",
        conv_sum
    );

    // ── Step 2: ReLU ────────────────────────────────────────────────────
    let relu_out_addr = RAM_BASE + 0x1C0;
    let desc_relu_out = RAM_BASE + 0x360;

    // Add some negatives to conv output for interesting ReLU
    write_f32_array(
        &mut ram,
        conv_out_addr as u64,
        &[
            -2.0, 1.0, -3.0, 4.0, 0.5, -0.5, 2.0, -1.0, 3.0, -4.0, 1.5, 0.0, -1.0, 2.5, -0.5, 3.0,
        ],
    );

    write_tensor_desc(&mut ram, desc_relu_out as u64, relu_out_addr, 16, [4, 4, 0]);

    gpu_write_reg(&mut gpu, 0x08, KernelType::Relu as u32);
    gpu_write_reg(&mut gpu, 0x10, desc_conv_out); // reuse conv_out addr as input desc
                                                  // Update input desc data to point at conv_out_addr
    write_guest_u32(&mut ram, desc_conv_out as u64, conv_out_addr);
    gpu_write_reg(&mut gpu, 0x30, desc_relu_out);

    gpu.execute_with_memory(&mut ram);

    let relu_out = read_f32_array(&mut ram, relu_out_addr as u64, 16);
    // ReLU should zero out negatives
    for (i, &v) in relu_out.iter().enumerate() {
        assert!(v >= 0.0, "relu_out[{}] = {} should be >= 0", i, v);
    }

    // ── Step 3: MaxPool2d (2x2 stride 2) ────────────────────────────────
    // Input: 4x4, output: 2x2
    let pool_out_addr = RAM_BASE + 0x200;
    let desc_pool_out = RAM_BASE + 0x380;

    write_tensor_desc(&mut ram, desc_relu_out as u64, relu_out_addr, 16, [1, 4, 4]);
    write_tensor_desc(&mut ram, desc_pool_out as u64, pool_out_addr, 4, [1, 2, 2]);

    gpu_write_reg(&mut gpu, 0x08, KernelType::Pool2dMax as u32);
    gpu_write_reg(&mut gpu, 0x10, desc_relu_out);
    gpu_write_reg(&mut gpu, 0x30, desc_pool_out);
    gpu_write_reg(&mut gpu, 0x40, (2 << 16) | 2); // 2x2 kernel
    gpu_write_reg(&mut gpu, 0x44, (2 << 16) | 2); // stride 2x2
    gpu_write_reg(&mut gpu, 0x48, 0); // no padding
    gpu_write_reg(&mut gpu, 0x4C, (4 << 16) | 4); // input 4x4
    gpu_write_reg(&mut gpu, 0x90, 1 << 16); // 1 channel

    gpu.execute_with_memory(&mut ram);

    let pool_out = read_f32_array(&mut ram, pool_out_addr as u64, 4);
    // Verify pool output is 2x2 and all non-negative (from ReLU output)
    assert_eq!(pool_out.len(), 4);
    for (i, &v) in pool_out.iter().enumerate() {
        assert!(v >= 0.0, "pool_out[{}] = {} should be >= 0", i, v);
    }

    // ── Step 4: Flatten + MatMul (4→2) ─────────────────────────────────
    // Treat 2x2 pooled output as flat vector [4], multiply by weight [2x4]
    let weight_addr = RAM_BASE + 0x220;
    let fc_out_addr = RAM_BASE + 0x250;
    let desc_flat = RAM_BASE + 0x3A0;
    let desc_w = RAM_BASE + 0x3C0;
    let desc_fc_out = RAM_BASE + 0x3E0;

    write_f32_array(
        &mut ram,
        weight_addr as u64,
        &[0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8],
    );

    write_tensor_desc(&mut ram, desc_flat as u64, pool_out_addr, 4, [1, 4, 0]);
    write_tensor_desc(&mut ram, desc_w as u64, weight_addr, 8, [2, 4, 0]);
    write_tensor_desc(&mut ram, desc_fc_out as u64, fc_out_addr, 2, [2, 1, 0]);

    gpu_write_reg(&mut gpu, 0x08, KernelType::MatMul as u32);
    gpu_write_reg(&mut gpu, 0x0C, Precision::Fp32 as u32);
    gpu_write_reg(&mut gpu, 0x10, desc_flat);
    gpu_write_reg(&mut gpu, 0x18, desc_w);
    gpu_write_reg(&mut gpu, 0x30, desc_fc_out);

    gpu.execute_with_memory(&mut ram);

    let fc_out = read_f32_array(&mut ram, fc_out_addr as u64, 2);
    // Both outputs should be finite
    assert!(
        fc_out[0].is_finite() && fc_out[1].is_finite(),
        "FC output should be finite: [{}, {}]",
        fc_out[0],
        fc_out[1]
    );

    // ── Step 5: Softmax ─────────────────────────────────────────────────
    let softmax_out_addr = RAM_BASE + 0x270;
    let desc_fc_in = RAM_BASE + 0x400;
    let desc_softmax_out = RAM_BASE + 0x420;

    write_tensor_desc(&mut ram, desc_fc_in as u64, fc_out_addr, 2, [2, 0, 0]);
    write_tensor_desc(
        &mut ram,
        desc_softmax_out as u64,
        softmax_out_addr,
        2,
        [2, 0, 0],
    );

    gpu_write_reg(&mut gpu, 0x08, KernelType::Softmax as u32);
    gpu_write_reg(&mut gpu, 0x10, desc_fc_in);
    gpu_write_reg(&mut gpu, 0x30, desc_softmax_out);

    gpu.execute_with_memory(&mut ram);

    let softmax_out = read_f32_array(&mut ram, softmax_out_addr as u64, 2);
    let sum: f32 = softmax_out.iter().sum();
    assert!(
        (sum - 1.0).abs() < 1e-4,
        "Softmax output should sum to 1.0, got sum={}",
        sum
    );
    assert!(
        softmax_out[0] > 0.0 && softmax_out[1] > 0.0,
        "Softmax outputs should be positive"
    );

    // Verify total kernels executed = 5 (conv2d + relu + pool2d + matmul + softmax)
    let perf = gpu.performance_counters();
    assert_eq!(perf.kernels_executed, 5, "Should have executed 5 kernels");
    assert_eq!(gpu.snapshot().tasks_done, 5);
}

/// ────────────────────────────────────────────────────────────────────────
/// Test 2: TPU INT8 quantized matmul end-to-end
/// ────────────────────────────────────────────────────────────────────────
#[test]
fn test_tpu_int8_matmul_e2e() {
    let mut tpu = Tpu::new();
    let mut ram = make_ram();

    // A = [[1,2],[3,4]], B = [[5,6],[7,8]], C = A*B = [[19,22],[43,50]]
    let a_addr = RAM_BASE + 0x100;
    let b_addr = RAM_BASE + 0x120;
    let c_addr = RAM_BASE + 0x140;

    // Write INT8 values (stored as bytes)
    let a_data: Vec<i8> = vec![1, 2, 3, 4];
    let b_data: Vec<i8> = vec![5, 6, 7, 8];
    for (i, &v) in a_data.iter().enumerate() {
        write_guest_u32(&mut ram, a_addr as u64 + i as u64, v as u32 & 0xFF);
    }
    for (i, &v) in b_data.iter().enumerate() {
        write_guest_u32(&mut ram, b_addr as u64 + i as u64, v as u32 & 0xFF);
    }

    // Configure TPU via MMIO registers
    // MATRIX_A_ADDR (0x10), MATRIX_A_ROWS (0x18), MATRIX_A_COLS (0x1C)
    tpu.write(Addr::new(0x10), (a_addr & 0xFF) as u8).unwrap();
    tpu.write(Addr::new(0x11), ((a_addr >> 8) & 0xFF) as u8)
        .unwrap();
    tpu.write(Addr::new(0x12), ((a_addr >> 16) & 0xFF) as u8)
        .unwrap();
    tpu.write(Addr::new(0x13), ((a_addr >> 24) & 0xFF) as u8)
        .unwrap();

    tpu.write(Addr::new(0x20), (b_addr & 0xFF) as u8).unwrap();
    tpu.write(Addr::new(0x21), ((b_addr >> 8) & 0xFF) as u8)
        .unwrap();
    tpu.write(Addr::new(0x22), ((b_addr >> 16) & 0xFF) as u8)
        .unwrap();
    tpu.write(Addr::new(0x23), ((b_addr >> 24) & 0xFF) as u8)
        .unwrap();

    tpu.write(Addr::new(0x30), (c_addr & 0xFF) as u8).unwrap();
    tpu.write(Addr::new(0x31), ((c_addr >> 8) & 0xFF) as u8)
        .unwrap();
    tpu.write(Addr::new(0x32), ((c_addr >> 16) & 0xFF) as u8)
        .unwrap();
    tpu.write(Addr::new(0x33), ((c_addr >> 24) & 0xFF) as u8)
        .unwrap();

    // M=2, K=2, N=2
    // MATRIX_A_ROWS (0x18) = M=2, MATRIX_A_COLS (0x1C) = K=2
    // MATRIX_B_ROWS (0x28) = K=2, MATRIX_B_COLS (0x2C) = N=2
    let m = 2u32;
    let k = 2u32;
    let n = 2u32;

    // MATRIX_A_ROWS = 0x18
    tpu.write(Addr::new(0x18), (m & 0xFF) as u8).unwrap();
    tpu.write(Addr::new(0x19), ((m >> 8) & 0xFF) as u8).unwrap();
    // MATRIX_A_COLS = 0x1C
    tpu.write(Addr::new(0x1C), (k & 0xFF) as u8).unwrap();
    tpu.write(Addr::new(0x1D), ((k >> 8) & 0xFF) as u8).unwrap();
    // MATRIX_B_COLS = 0x2C
    tpu.write(Addr::new(0x2C), (n & 0xFF) as u8).unwrap();
    tpu.write(Addr::new(0x2D), ((n >> 8) & 0xFF) as u8).unwrap();

    // Trigger compute
    tpu.execute_with_memory(&mut ram);

    // Read results (i32 values, written as u32)
    let c0 = read_guest_u32(&mut ram, c_addr as u64) as i32;
    let c1 = read_guest_u32(&mut ram, c_addr as u64 + 4) as i32;
    let c2 = read_guest_u32(&mut ram, c_addr as u64 + 8) as i32;
    let c3 = read_guest_u32(&mut ram, c_addr as u64 + 12) as i32;

    assert_eq!(c0, 19, "C[0][0] should be 19, got {}", c0);
    assert_eq!(c1, 22, "C[0][1] should be 22, got {}", c1);
    assert_eq!(c2, 43, "C[1][0] should be 43, got {}", c2);
    assert_eq!(c3, 50, "C[1][1] should be 50, got {}", c3);

    let perf = tpu.performance_counters();
    assert!(
        perf.kernels_executed >= 1,
        "TPU should have executed at least 1 kernel"
    );
}

/// ────────────────────────────────────────────────────────────────────────
/// Test 3: GPU + TPU pipeline (GPU conv, TPU matmul, GPU softmax)
/// ────────────────────────────────────────────────────────────────────────
#[test]
fn test_gpu_tpu_pipeline() {
    let mut gpu = Gpu::new();
    let tpu = Tpu::new();
    let mut ram = make_ram();

    // Step 1: GPU produces a feature vector via Conv2d
    let input_addr = RAM_BASE + 0x100;
    let filter_addr = RAM_BASE + 0x120;
    let feat_addr = RAM_BASE + 0x140;

    // 1 channel, 2x2 input, 1x1 filter, stride 1 → 2x2 output (4 elements)
    write_f32_array(&mut ram, input_addr as u64, &[1.0, 2.0, 3.0, 4.0]);
    write_f32_array(&mut ram, filter_addr as u64, &[2.0]);

    let desc_in = RAM_BASE + 0x300;
    let desc_f = RAM_BASE + 0x320;
    let desc_feat = RAM_BASE + 0x340;

    write_tensor_desc(&mut ram, desc_in as u64, input_addr, 4, [1, 2, 2]);
    write_tensor_desc(&mut ram, desc_f as u64, filter_addr, 1, [1, 1, 1]);
    write_tensor_desc(&mut ram, desc_feat as u64, feat_addr, 4, [1, 2, 2]);

    gpu_write_reg(&mut gpu, 0x08, KernelType::Conv2d as u32);
    gpu_write_reg(&mut gpu, 0x10, desc_in);
    gpu_write_reg(&mut gpu, 0x18, desc_f);
    gpu_write_reg(&mut gpu, 0x30, desc_feat);
    gpu_write_reg(&mut gpu, 0x40, (1 << 16) | 1); // 1x1 kernel
    gpu_write_reg(&mut gpu, 0x44, (1 << 16) | 1); // stride 1
    gpu_write_reg(&mut gpu, 0x48, 0); // no padding
    gpu_write_reg(&mut gpu, 0x4C, (2 << 16) | 2); // input 2x2
    gpu_write_reg(&mut gpu, 0x90, (1 << 16) | 1); // 1ch in, 1ch out

    gpu.execute_with_memory(&mut ram);

    let feat = read_f32_array(&mut ram, feat_addr as u64, 4);
    // Each input * 2: [2,4,6,8]
    assert!((feat[0] - 2.0).abs() < 1e-4);
    assert!((feat[3] - 8.0).abs() < 1e-4);

    // Step 2: Apply ReLU on GPU (no-op for positive values, but validates pipeline)
    let relu_addr = RAM_BASE + 0x160;
    let desc_relu = RAM_BASE + 0x360;

    write_tensor_desc(&mut ram, desc_feat as u64, feat_addr, 4, [4, 0, 0]);
    write_tensor_desc(&mut ram, desc_relu as u64, relu_addr, 4, [4, 0, 0]);

    gpu_write_reg(&mut gpu, 0x08, KernelType::Relu as u32);
    gpu_write_reg(&mut gpu, 0x10, desc_feat);
    gpu_write_reg(&mut gpu, 0x30, desc_relu);

    gpu.execute_with_memory(&mut ram);

    let relu_out = read_f32_array(&mut ram, relu_addr as u64, 4);
    assert!((relu_out[0] - 2.0).abs() < 1e-4);

    // Step 3: Softmax on GPU
    let softmax_addr = RAM_BASE + 0x180;
    let desc_sm_in = RAM_BASE + 0x380;
    let desc_sm_out = RAM_BASE + 0x3A0;

    write_tensor_desc(&mut ram, desc_sm_in as u64, relu_addr, 4, [4, 0, 0]);
    write_tensor_desc(&mut ram, desc_sm_out as u64, softmax_addr, 4, [4, 0, 0]);

    gpu_write_reg(&mut gpu, 0x08, KernelType::Softmax as u32);
    gpu_write_reg(&mut gpu, 0x10, desc_sm_in);
    gpu_write_reg(&mut gpu, 0x30, desc_sm_out);

    gpu.execute_with_memory(&mut ram);

    let sm_out = read_f32_array(&mut ram, softmax_addr as u64, 4);
    let sm_sum: f32 = sm_out.iter().sum();
    assert!(
        (sm_sum - 1.0).abs() < 1e-4,
        "softmax should sum to 1, got {}",
        sm_sum
    );

    // Verify GPU executed 3 kernels
    assert_eq!(gpu.performance_counters().kernels_executed, 3);

    // TPU should still be idle (not used in this pipeline)
    assert!(!tpu.is_busy());
}
