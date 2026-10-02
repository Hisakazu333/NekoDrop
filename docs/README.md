# NekoDrop 文档索引

NekoDrop 是 NekoLink 协议的第一个桌面落地项目：macOS / Windows 局域网安全文件传输。

文档按读者分组组织。改文档时请保持"链接进索引、索引不失真"。

## 用户与发布

- [产品定义](product/PRODUCT.md)：NekoDrop 做什么、不做什么。
- [当前状态](product/STATUS.md)：什么已实现、什么是实验性、什么在计划中（状态的唯一事实来源）。
- [路线图](product/ROADMAP.md)：按版本推进的阶段计划。

## 开发者

- [开发指南](dev/DEVELOPMENT.md)：环境搭建、构建、测试、打包、日常流程。
- [架构](dev/ARCHITECTURE.md)：workspace 布局与职责边界。
- [模块](dev/MODULES.md)：模块所有权与依赖方向；分模块文档见 [dev/modules/](dev/modules/)。
- [协议](dev/PROTOCOL.md)：NekoLink 信封、传输流程、TCP 文件帧。
- [Bundle 规范](dev/BUNDLE_SPEC.md)：资料包 manifest、checksums、权限、暂存与导入。
- [Adapter 规范](dev/ADAPTER_SPEC.md)：本机应用接入 local bridge 的请求流程；可执行示例见 [examples/generic-adapter/](examples/generic-adapter/)。
- [安全模型](dev/SECURITY.md)：信任、配对、接收安全与已知边界。
- [代码审计](dev/audit-2026-10-03.md)：2026-10-03 全库审计报告与整改进度。

## 测试

- [大文件传输测试矩阵](testing/LARGE_FILE_TRANSFER_MATRIX.md)：Mac / Windows 发布前手工验证清单。
- [测试结果模板](testing/RESULT_TEMPLATE.md)：发布候选测试记录模板。

## 示例与样例数据

- [generic-adapter 示例](examples/generic-adapter/)：local bridge 通用请求顺序的可执行样板。
- [bundle-samples](bundle-samples/)：session / skill / workspace 等资料包样例数据。

## 历史归档

早期规划与设计笔记，仅作参考，不代表当前方向：

- [archive/](archive/)：未来迭代计划、下一阶段分析、旧桌面设计稿、2026 年 6 月的 specs/plans。

## 状态标签约定

全文统一使用以下标签，避免把未完成能力写成已完成：

- `Implemented`：代码存在，当前桌面应用可用。
- `Experimental`：代码或接口存在，但还不是受支持的用户流程。
- `Planned`：只有产品或协议方向。
- `Out of scope`：明确不在当前阶段。
