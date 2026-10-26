
//! 内置字体，只在这里嵌入一次，避免被多处 include_bytes! 重复打进二进制。
pub static NOTO_SERIF_SC_REGULAR: &[u8] =
    include_bytes!("../assets/fonts/NotoSerifSC-Regular.ttf");
pub static NOTO_SERIF_SC_SEMIBOLD: &[u8] =
    include_bytes!("../assets/fonts/NotoSerifSC-SemiBold.ttf");
