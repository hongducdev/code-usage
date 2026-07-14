# Claude-inspired Design System

> Tài liệu tham chiếu để tạo giao diện mang cảm giác gần với Claude/Anthropic. Không phải brand guideline chính thức. Màu, font và token bên dưới được tổng hợp từ giao diện/cách trình bày công khai của Anthropic và Claude; giá trị có thể đổi theo phiên bản sản phẩm.

## 1. Tinh thần thiết kế

Claude dùng phong cách ấm, điềm tĩnh, trí tuệ, tối giản.

- Nền trắng ngà thay vì trắng tinh.
- Chữ gần đen, tương phản cao nhưng không gắt.
- Màu cam đất làm điểm nhấn.
- Bo góc mềm, viền mảnh, bóng rất nhẹ.
- Nhiều khoảng thở.
- Typography thiên về biên tập: serif cho tiêu đề/nội dung giàu tính đọc; sans-serif cho điều khiển, nhãn, metadata.
- Chuyển động ngắn, nhẹ, không phô trương.

## 2. Bảng màu

### Màu cốt lõi

| Token | Giá trị | Dùng cho |
|---|---:|---|
| `--color-bg` | `#F7F6F2` | Nền trang chính |
| `--color-surface` | `#FFFFFF` | Card, panel, modal |
| `--color-surface-muted` | `#F1EFE8` | Khối phụ, vùng nhập, hover nhẹ |
| `--color-text` | `#181817` | Chữ chính |
| `--color-text-muted` | `#686762` | Chữ phụ, metadata |
| `--color-border` | `#D8D5CC` | Viền mặc định |
| `--color-border-strong` | `#B8B4AA` | Viền focus/active nhẹ |
| `--color-accent` | `#D97757` | CTA, link nổi bật, trạng thái active |
| `--color-accent-hover` | `#C96546` | Hover của accent |
| `--color-accent-soft` | `#F3DED5` | Nền badge hoặc vùng nhấn nhẹ |

### Màu trạng thái

| Token | Giá trị | Dùng cho |
|---|---:|---|
| `--color-success` | `#3F7D58` | Thành công |
| `--color-success-soft` | `#E4F0E7` | Nền thành công |
| `--color-warning` | `#A36B21` | Cảnh báo |
| `--color-warning-soft` | `#F5E8CF` | Nền cảnh báo |
| `--color-danger` | `#B84A45` | Lỗi/xóa |
| `--color-danger-soft` | `#F5DEDC` | Nền lỗi |
| `--color-info` | `#4774A8` | Thông tin |
| `--color-info-soft` | `#E0EAF4` | Nền thông tin |

### Dark mode gợi ý

| Token | Giá trị |
|---|---:|
| `--color-bg-dark` | `#1F1E1B` |
| `--color-surface-dark` | `#292824` |
| `--color-surface-muted-dark` | `#33312C` |
| `--color-text-dark` | `#F2F0E9` |
| `--color-text-muted-dark` | `#B8B4AA` |
| `--color-border-dark` | `#45423C` |
| `--color-accent-dark` | `#E08A6D` |

## 3. Font chữ

Claude/Anthropic có dấu ấn typography biên tập, thường phối serif và sans-serif. Font thương hiệu chính xác có thể là font thương mại hoặc font nội bộ. Dùng stack thay thế sau để đạt cảm giác gần nhất.

### Serif: tiêu đề và nội dung dài

```css
font-family: "Tiempos Text", "Source Serif 4", "Iowan Old Style",
  "Palatino Linotype", Georgia, serif;
```

Dùng cho:

- `h1`, `h2` quan trọng.
- Đoạn trả lời dài.
- Trích dẫn.
- Nội dung mang tính biên tập.

### Sans-serif: UI và điều khiển

```css
font-family: "Styrene", "Inter", "Helvetica Neue", Arial, sans-serif;
```

Dùng cho:

- Button.
- Input.
- Navigation.
- Badge.
- Nhãn và metadata.

### Monospace: code

```css
font-family: "SFMono-Regular", "Cascadia Code", "Roboto Mono",
  Consolas, monospace;
```

### Thang chữ

| Token | Cỡ chữ | Line-height | Weight |
|---|---:|---:|---:|
| `--text-xs` | `12px` | `16px` | `400–500` |
| `--text-sm` | `14px` | `20px` | `400–500` |
| `--text-base` | `16px` | `26px` | `400` |
| `--text-lg` | `18px` | `28px` | `400–500` |
| `--text-xl` | `22px` | `30px` | `500–600` |
| `--text-2xl` | `28px` | `36px` | `500–600` |
| `--text-3xl` | `38px` | `46px` | `500–600` |
| `--text-4xl` | `52px` | `58px` | `500–600` |

Quy tắc:

- Body dài: `16–18px`, line-height `1.6–1.75`.
- Tiêu đề serif: weight vừa, không quá đậm.
- UI sans-serif: letter-spacing từ `-0.01em` đến `0`.
- Label viết hoa: dùng ít; `letter-spacing: 0.04em`.

## 4. Khoảng cách và bố cục

Dùng hệ 4px.

```css
--space-1: 4px;
--space-2: 8px;
--space-3: 12px;
--space-4: 16px;
--space-5: 20px;
--space-6: 24px;
--space-8: 32px;
--space-10: 40px;
--space-12: 48px;
--space-16: 64px;
```

- Chiều rộng vùng đọc: `680–760px`.
- Dashboard rộng: `1120–1280px`.
- Padding card: `16–24px`.
- Khoảng cách section: `48–80px`.
- Sidebar: `260–300px`.

## 5. Bo góc, viền, bóng

```css
--radius-sm: 6px;
--radius-md: 10px;
--radius-lg: 14px;
--radius-xl: 20px;
--radius-pill: 999px;

--border-default: 1px solid #D8D5CC;
--shadow-sm: 0 1px 2px rgb(24 24 23 / 0.06);
--shadow-md: 0 8px 24px rgb(24 24 23 / 0.08);
```

Quy tắc:

- Card thường dùng viền; tránh bóng nặng.
- Input và composer bo `12–18px`.
- Button bo `8–10px` hoặc pill cho action nhỏ.
- Modal dùng `16–20px`.

## 6. Component style

### Button chính

```css
.button-primary {
  min-height: 40px;
  padding: 0 16px;
  border: 1px solid transparent;
  border-radius: 10px;
  background: #181817;
  color: #ffffff;
  font: 500 14px/1 "Inter", sans-serif;
  transition: background-color 140ms ease, transform 140ms ease;
}

.button-primary:hover {
  background: #302f2c;
}

.button-primary:active {
  transform: translateY(1px);
}
```

### Button accent

```css
.button-accent {
  min-height: 40px;
  padding: 0 16px;
  border: 1px solid transparent;
  border-radius: 10px;
  background: #D97757;
  color: #ffffff;
  font: 500 14px/1 "Inter", sans-serif;
}

.button-accent:hover {
  background: #C96546;
}
```

### Button phụ

```css
.button-secondary {
  min-height: 40px;
  padding: 0 16px;
  border: 1px solid #D8D5CC;
  border-radius: 10px;
  background: #FFFFFF;
  color: #181817;
  font: 500 14px/1 "Inter", sans-serif;
}

.button-secondary:hover {
  background: #F1EFE8;
}
```

### Input / prompt composer

```css
.composer {
  border: 1px solid #D8D5CC;
  border-radius: 18px;
  background: #FFFFFF;
  box-shadow: 0 1px 2px rgb(24 24 23 / 0.06);
  padding: 14px 16px;
}

.composer:focus-within {
  border-color: #A9A59B;
  box-shadow: 0 0 0 3px rgb(217 119 87 / 0.14);
}
```

### Card

```css
.card {
  border: 1px solid #D8D5CC;
  border-radius: 14px;
  background: #FFFFFF;
  padding: 20px;
}
```

### Code block

```css
.code-block {
  border: 1px solid #D8D5CC;
  border-radius: 10px;
  background: #F1EFE8;
  padding: 16px;
  overflow-x: auto;
  font: 400 13px/1.65 "SFMono-Regular", monospace;
}
```

## 7. Icon và hình ảnh

- Icon nét mảnh, hình học đơn giản.
- Stroke khoảng `1.5–2px`.
- Kích thước phổ biến: `16`, `18`, `20`, `24px`.
- Tránh icon 3D bóng, gradient mạnh.
- Hình minh họa nên có chất liệu thủ công, hình học, khoa học hoặc tự nhiên.
- Màu ảnh trầm, ấm, ít bão hòa.

## 8. Motion

```css
--duration-fast: 120ms;
--duration-base: 180ms;
--duration-slow: 260ms;
--ease-standard: cubic-bezier(0.2, 0, 0, 1);
```

- Hover: `120–180ms`.
- Modal/dropdown: `180–260ms`.
- Dùng fade, scale rất nhẹ, translate `2–6px`.
- Không bounce.
- Tôn trọng `prefers-reduced-motion`.

## 9. Giọng điệu UI

- Bình tĩnh, rõ, trực tiếp.
- Sentence case.
- Tránh dấu chấm than.
- CTA dùng động từ ngắn: `Create`, `Continue`, `Try again`, `Share`.
- Error nói rõ vấn đề và bước sửa.
- Không dùng quá nhiều badge hoặc màu trạng thái.

## 10. CSS tokens hoàn chỉnh

```css
:root {
  --color-bg: #F7F6F2;
  --color-surface: #FFFFFF;
  --color-surface-muted: #F1EFE8;
  --color-text: #181817;
  --color-text-muted: #686762;
  --color-border: #D8D5CC;
  --color-border-strong: #B8B4AA;
  --color-accent: #D97757;
  --color-accent-hover: #C96546;
  --color-accent-soft: #F3DED5;

  --font-serif: "Tiempos Text", "Source Serif 4", "Iowan Old Style",
    "Palatino Linotype", Georgia, serif;
  --font-sans: "Styrene", "Inter", "Helvetica Neue", Arial, sans-serif;
  --font-mono: "SFMono-Regular", "Cascadia Code", "Roboto Mono",
    Consolas, monospace;

  --radius-sm: 6px;
  --radius-md: 10px;
  --radius-lg: 14px;
  --radius-xl: 20px;
  --radius-pill: 999px;

  --space-1: 4px;
  --space-2: 8px;
  --space-3: 12px;
  --space-4: 16px;
  --space-5: 20px;
  --space-6: 24px;
  --space-8: 32px;
  --space-10: 40px;
  --space-12: 48px;
  --space-16: 64px;

  --shadow-sm: 0 1px 2px rgb(24 24 23 / 0.06);
  --shadow-md: 0 8px 24px rgb(24 24 23 / 0.08);

  --duration-fast: 120ms;
  --duration-base: 180ms;
  --duration-slow: 260ms;
  --ease-standard: cubic-bezier(0.2, 0, 0, 1);
}
```

## 11. Không nên dùng

- Trắng tinh phủ toàn bộ giao diện.
- Đen tuyệt đối `#000000` cho vùng lớn.
- Gradient neon.
- Bóng sâu kiểu glassmorphism.
- Bo góc quá lớn trên mọi component.
- Font geometric sans quá lạnh cho toàn bộ nội dung.
- Heading quá đậm.
- Mật độ UI cao, ít khoảng thở.
- Nhiều màu accent cạnh tranh nhau.

## 12. Nguồn tham chiếu

- Anthropic: https://www.anthropic.com/
- Anthropic Newsroom và press kit: https://www.anthropic.com/news
- Claude: https://claude.ai/

Tài liệu cập nhật ngày 2026-07-14. Anthropic không công bố đầy đủ token UI của Claude trong nguồn công khai được kiểm tra; font và mã màu cụ thể nên xem là giá trị gần đúng, dùng cho thiết kế “Claude-inspired”, không phải bản sao thương hiệu chính thức.
