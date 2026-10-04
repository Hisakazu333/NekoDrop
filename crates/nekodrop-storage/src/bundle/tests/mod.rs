// bundle 测试按行为拆分：创建与识别、暂存与列表、清理、导入、回滚；
// 共享夹具集中在 `fixtures`。
mod cleanup;
mod create_detect;
mod fixtures;
mod import;
mod rollback;
mod staging;
