//! OpenAI Responses API → Chat Completions 请求转换。
//! 按职责拆为子模块:convert(入口与请求组装)、input(input items 提取)、
//! think(think 标签切分)、effort(effort 覆盖)。对外 API 经此处重导出,路径不变。

mod convert;
mod effort;
mod input;
mod think;

pub use convert::{convert_with_provider, convert_with_provider_matrix};
pub use input::flatten_tool_output;
pub use think::{split_think_tags, ThinkSplitter};

#[cfg(test)]
mod tests;
