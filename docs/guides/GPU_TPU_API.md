# GPU/TPU 模拟加速器 — 软件 API 文档

> 本文档描述 myCPU 模拟器中 GPU 和 TPU 外设的 MMIO 寄存器接口，
> 供 guest 软件驱动开发使用。

---

## 1. 地址映射

| 外设 | 基地址       | 大小   | IRQ 源 |
|------|-------------|--------|--------|
| GPU  | `0x2001_0000` | 4 KB | PLIC #13 |
| TPU  | `0x2002_0000` | 4 KB | PLIC #14 |

---

## 2. GPU 寄存器表

### 2.1 控制与状态

| 偏移   | 名称            | 读/写 | 说明                                      |
|--------|----------------|-------|-------------------------------------------|
| `0x00` | CONTROL        | RW    | 控制寄存器（见位定义）                      |
| `0x04` | STATUS         | RW¹   | 状态寄存器（W1C：写 1 清除对应位）           |
| `0x08` | KERNEL_TYPE    | RW    | 内核类型（见 KernelType 枚举）               |
| `0x0C` | PRECISION      | RW    | 数据精度（见 Precision 枚举）                |

#### CONTROL 位定义

| Bit | 名称           | 说明                                    |
|-----|---------------|-----------------------------------------|
| 0   | START         | 写 1 启动一次计算（自动清零）              |
| 1   | RESET         | 写 1 复位 GPU 状态（自动清零）             |
| 2   | IRQ_EN        | 中断使能                                  |

#### STATUS 位定义（W1C）

| Bit | 名称         | 说明                    |
|-----|-------------|-------------------------|
| 0   | BUSY        | 正在执行                 |
| 1   | DONE        | 执行完成                 |
| 2   | ERROR       | 执行出错                 |
| 3   | IRQ_PENDING | 中断待处理               |

### 2.2 张量描述符地址

| 偏移       | 名称              | 说明                     |
|-----------|-------------------|--------------------------|
| `0x10`    | INPUT0_DESC_LOW   | 输入张量 0 描述符地址低 32 位 |
| `0x14`    | INPUT0_DESC_HIGH  | 输入张量 0 描述符地址高 32 位 |
| `0x18`    | INPUT1_DESC_LOW   | 输入张量 1 描述符地址低 32 位 |
| `0x1C`    | INPUT1_DESC_HIGH  | 输入张量 1 描述符地址高 32 位 |
| `0x20`    | INPUT2_DESC_LOW   | 输入张量 2 描述符地址低 32 位 |
| `0x24`    | INPUT2_DESC_HIGH  | 输入张量 2 描述符地址高 32 位 |
| `0x30`    | OUTPUT0_DESC_LOW  | 输出张量 0 描述符地址低 32 位 |
| `0x34`    | OUTPUT0_DESC_HIGH | 输出张量 0 描述符地址高 32 位 |
| `0x38`    | OUTPUT1_DESC_LOW  | 输出张量 1 描述符地址低 32 位 |
| `0x3C`    | OUTPUT1_DESC_HIGH | 输出张量 1 描述符地址高 32 位 |

### 2.3 Conv2d / Pool2d 参数

| 偏移   | 名称             | 格式                      | 说明           |
|--------|-----------------|---------------------------|---------------|
| `0x40` | CONV_KERNEL_SIZE| `(kh << 16) \| kw`        | 卷积核尺寸     |
| `0x44` | CONV_STRIDE     | `(stride_h << 16) \| stride_w` | 步幅      |
| `0x48` | CONV_PADDING    | `(pad_h << 16) \| pad_w`  | 填充           |
| `0x4C` | CONV_INPUT_DIMS | `(in_h << 16) \| in_w`    | 输入空间尺寸   |
| `0x90` | CONV_CHANNELS   | `(c_in << 16) \| c_out`   | 输入/输出通道数 |

> Conv2d 使用 `INPUT0` 为输入数据、`INPUT1` 为滤波器权重、`OUTPUT0` 为输出。
> Pool2d 使用 `INPUT0` 为输入、`OUTPUT0` 为输出。CONV_CHANNELS 高 16 位为通道数。

### 2.4 命令队列（批量提交）

| 偏移   | 名称              | 说明                     |
|--------|------------------|--------------------------|
| `0x50` | CMD_QUEUE_BASE_LOW  | 命令队列基址低 32 位     |
| `0x54` | CMD_QUEUE_BASE_HIGH | 命令队列基址高 32 位     |
| `0x58` | CMD_QUEUE_LEN    | 命令数量                   |
| `0x5C` | CMD_QUEUE_NOTIFY | 写非零触发批量执行          |

每条命令描述符 16 字节：

| 偏移   | 字段               | 说明             |
|--------|-------------------|-----------------|
| `+0`   | kernel_type (u32) | 内核类型         |
| `+4`   | precision (u32)   | 数据精度         |
| `+8`   | input0_desc (u32) | 输入描述符地址    |
| `+12`  | output0_desc (u32)| 输出描述符地址    |

### 2.5 性能计数器

| 偏移   | 名称                 | 说明           |
|--------|---------------------|---------------|
| `0x60` | KERNELS_EXECUTED_LOW | 已执行内核数低 32 位 |
| `0x64` | KERNELS_EXECUTED_HIGH| 已执行内核数高 32 位 |
| `0x68` | CYCLES_LOW          | 模拟周期低 32 位   |
| `0x6C` | CYCLES_HIGH         | 模拟周期高 32 位   |
| `0x70` | OPS_COUNT_LOW       | 运算数低 32 位     |
| `0x74` | OPS_COUNT_HIGH      | 运算数高 32 位     |
| `0x78` | BYTES_TRANSFERRED_LOW | 传输字节低 32 位 |
| `0x7C` | BYTES_TRANSFERRED_HIGH| 传输字节高 32 位 |

### 2.6 错误与任务统计

| 偏移   | 名称        | 说明                |
|--------|-------------|--------------------|
| `0x80` | ERROR_CODE  | 错误码（0=无错）     |
| `0x84` | TASKS_DONE  | 已完成任务数         |
| `0x88` | TASKS_ERROR | 失败任务数          |

---

## 3. TPU 寄存器表

### 3.1 控制与状态

| 偏移   | 名称         | 读/写 | 说明                     |
|--------|-------------|-------|------------------------|
| `0x00` | CONTROL     | RW    | 控制寄存器（同 GPU 位定义） |
| `0x04` | STATUS      | RW¹   | 状态寄存器               |
| `0x08` | KERNEL_TYPE | RW    | 内核类型                 |

### 3.2 矩阵配置

| 偏移   | 名称              | 说明                     |
|--------|------------------|--------------------------|
| `0x10` | MATRIX_A_ADDR_LOW | 矩阵 A 地址低 32 位      |
| `0x14` | MATRIX_A_ADDR_HIGH| 矩阵 A 地址高 32 位      |
| `0x18` | MATRIX_A_ROWS     | 矩阵 A 行数 (M)           |
| `0x1C` | MATRIX_A_COLS     | 矩阵 A 列数 (K)           |
| `0x20` | MATRIX_B_ADDR_LOW | 矩阵 B 地址低 32 位      |
| `0x24` | MATRIX_B_ADDR_HIGH| 矩阵 B 地址高 32 位      |
| `0x28` | MATRIX_B_ROWS     | 矩阵 B 行数 (K)           |
| `0x2C` | MATRIX_B_COLS     | 矩阵 B 列数 (N)           |
| `0x30` | MATRIX_C_ADDR_LOW | 结果矩阵 C 地址低 32 位  |
| `0x34` | MATRIX_C_ADDR_HIGH| 结果矩阵 C 地址高 32 位  |

### 3.3 量化参数

| 偏移   | 名称              | 说明                              |
|--------|------------------|-----------------------------------|
| `0x40` | INPUT_SCALE      | 输入缩放因子（IEEE 754 float 位模式 u32） |
| `0x44` | INPUT_ZERO_POINT | 输入零点（i32）                     |
| `0x48` | OUTPUT_SCALE     | 输出缩放因子（IEEE 754 float 位模式 u32）|
| `0x4C` | OUTPUT_ZERO_POINT| 输出零点（i32）                     |

> 默认值：input_scale = output_scale = 1.0f（`0x3F800000`），zero_point = 0。

### 3.4 性能计数器与错误

| 偏移   | 名称               | 说明              |
|--------|-------------------|------------------|
| `0x60` | MATRICES_COMPUTED_LOW | 已计算矩阵数低 32 位 |
| `0x64` | MATRICES_COMPUTED_HIGH| 已计算矩阵数高 32 位 |
| `0x68` | CYCLES_LOW        | 模拟周期低 32 位    |
| `0x6C` | CYCLES_HIGH       | 模拟周期高 32 位    |
| `0x70` | OPS_COUNT_LOW     | 运算数低 32 位      |
| `0x74` | OPS_COUNT_HIGH    | 运算数高 32 位      |
| `0x90` | ERROR_CODE        | 错误码             |
| `0x94` | TASKS_DONE        | 已完成任务数        |
| `0x98` | TASKS_ERROR       | 失败任务数         |

---

## 4. 张量描述符格式

GPU 使用张量描述符传递数据位置和形状信息。描述符为 20 字节结构：

| 偏移   | 字段            | 类型 | 说明                |
|--------|----------------|------|-------------------|
| `+0`   | data_addr      | u32  | 数据在 guest 内存中的地址 |
| `+4`   | element_count  | u32  | 元素总数             |
| `+8`   | shape[0]       | u32  | 维度 0              |
| `+12`  | shape[1]       | u32  | 维度 1              |
| `+16`  | shape[2]       | u32  | 维度 2              |

### shape 语义

| 内核       | INPUT0 shape          | INPUT1 shape          | OUTPUT0 shape         |
|-----------|----------------------|-----------------------|-----------------------|
| MatMul    | `[M, K, 0]`          | `[K, N, 0]`           | `[M, N, 0]`           |
| VectorAdd | `[N, 0, 0]`          | `[N, 0, 0]`           | `[N, 0, 0]`           |
| Relu      | `[N, 0, 0]`          | —                     | `[N, 0, 0]`           |
| Softmax   | `[N, 0, 0]`          | —                     | `[N, 0, 0]`           |
| Conv2d    | `[C_in, H, W]`       | `[C_out, C_in*kh*kw]` | `[C_out, H_out, W_out]`|
| Pool2dMax | `[C, H, W]`          | —                     | `[C, H_out, W_out]`   |
| Pool2dAvg | `[C, H, W]`          | —                     | `[C, H_out, W_out]`   |

---

## 5. KernelType 枚举

| 值   | 名称            | GPU | TPU |
|------|----------------|-----|-----|
| 0    | MatMul         | ✅  | ✅  |
| 1    | MatAdd         | ✅  | —   |
| 10   | Conv2d         | ✅  | —   |
| 12   | Pool2dMax      | ✅  | —   |
| 13   | Pool2dAvg      | ✅  | —   |
| 20   | VectorAdd      | ✅  | —   |
| 21   | VectorMul      | ✅  | —   |
| 22   | VectorDot      | ✅  | —   |
| 23   | VectorScale    | ✅  | —   |
| 30   | Relu           | ✅  | —   |
| 31   | Relu6          | ✅  | —   |
| 32   | Sigmoid        | ✅  | —   |
| 33   | Tanh           | ✅  | —   |
| 34   | Softmax        | ✅  | —   |
| 35   | LeakyRelu      | ✅  | —   |

## 6. Precision 枚举

| 值 | 名称  | 字节/元素 |
|----|-------|----------|
| 0  | FP32  | 4        |
| 1  | FP16  | 2        |
| 2  | INT8  | 1        |
| 3  | INT32 | 4        |
| 4  | BF16  | 2        |

---

## 7. 使用示例

### 7.1 GPU 单次 MatMul

```c
// 伪代码：C = A * B (2x2 * 2x2 = 2x2)
#define GPU_BASE  0x20010000

// 1. 在 RAM 中准备输入/输出数据和描述符
struct TensorDesc {
    uint32_t data_addr;
    uint32_t element_count;
    uint32_t shape[3];
};

struct TensorDesc desc_a = { 0x80000100, 4, {2, 2, 0} };
struct TensorDesc desc_b = { 0x80000120, 4, {2, 2, 0} };
struct TensorDesc desc_c = { 0x80000140, 4, {2, 2, 0} };

// 2. 配置 GPU 寄存器
mmio_write(GPU_BASE + 0x08, 0);     // KERNEL_TYPE = MatMul
mmio_write(GPU_BASE + 0x0C, 0);     // PRECISION  = FP32
mmio_write(GPU_BASE + 0x10, &desc_a); // INPUT0_DESC
mmio_write(GPU_BASE + 0x18, &desc_b); // INPUT1_DESC
mmio_write(GPU_BASE + 0x30, &desc_c); // OUTPUT0_DESC

// 3. 启动计算
mmio_write(GPU_BASE + 0x00, 0x05);  // CONTROL: START=1, IRQ_EN=1

// 4. 等待完成（轮询或中断）
while (!(mmio_read(GPU_BASE + 0x04) & 0x02)); // wait for DONE
```

### 7.2 GPU Conv2d → ReLU 流水线

```c
// Step 1: Conv2d
mmio_write(GPU_BASE + 0x08, 10);    // KERNEL_TYPE = Conv2d
mmio_write(GPU_BASE + 0x40, (3<<16)|3); // CONV_KERNEL_SIZE = 3x3
mmio_write(GPU_BASE + 0x44, (1<<16)|1); // CONV_STRIDE = 1
mmio_write(GPU_BASE + 0x48, (1<<16)|1); // CONV_PADDING = 1
mmio_write(GPU_BASE + 0x4C, (8<<16)|8); // CONV_INPUT_DIMS = 8x8
mmio_write(GPU_BASE + 0x90, (3<<16)|16); // 3 input channels, 16 output filters
mmio_write(GPU_BASE + 0x00, 1);     // START
// ... wait for DONE ...

// Step 2: ReLU（复用 conv 输出描述符）
mmio_write(GPU_BASE + 0x08, 30);    // KERNEL_TYPE = Relu
mmio_write(GPU_BASE + 0x00, 1);     // START
```

### 7.3 TPU INT8 量化矩阵乘

```c
#define TPU_BASE  0x20020000

mmio_write(TPU_BASE + 0x10, addr_a);  // MATRIX_A_ADDR
mmio_write(TPU_BASE + 0x18, 64);      // MATRIX_A_ROWS (M=64)
mmio_write(TPU_BASE + 0x1C, 128);     // MATRIX_A_COLS (K=128)
mmio_write(TPU_BASE + 0x20, addr_b);  // MATRIX_B_ADDR
mmio_write(TPU_BASE + 0x2C, 32);      // MATRIX_B_COLS (N=32)
mmio_write(TPU_BASE + 0x30, addr_c);  // MATRIX_C_ADDR
mmio_write(TPU_BASE + 0x00, 1);       // START
```

---

## 8. 错误码

| 码 | 含义                         |
|----|------------------------------|
| 0  | 无错误                       |
| 1  | 不支持的内核类型             |
| 2  | 执行错误（内存越界等）        |
| 3  | 无效参数（Conv2d/Pool2d 参数为零）|
