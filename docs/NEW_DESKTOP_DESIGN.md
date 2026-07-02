# NekoDrop 桌面客户端全新工具化设计方案

本方案抛弃了花哨、拟物和游戏化的交互设计，采用类似 **Obsidian / VS Code / GitHub Desktop** 的**“三栏式工具工作台 (Three-Column Workbench)”**布局。该设计结构规整、模块化强，且在 React + Tauri 中**开发成本极低**，非常适合高效率的生产力工具定位。

![NekoDrop 全新工具化 UI 设计概念图](file:///Users/hisakazu/.gemini/antigravity/brain/34ac6590-7d87-4c76-963c-0eb002bc1c02/nekodrop_workbench_ui_mockup_1782580263412.jpg)

---

## 一、 经典三栏式布局结构

新版界面使用 `1px` 细线进行严格的区域切分，整体采用无边框暗黑工业风：

### 1. 左栏：设备与网络树 (Devices & Network)
*   **导航条**：最左侧为极窄的活动栏（Activity Bar），提供文件、设备、历史和设置的快速切换。
*   **设备树**：包含两个标准的折叠列表（Collapsible List）：
    *   **附近设备 (Nearby)**：局域网中新发现的待配对设备。
    *   **可信设备 (Trusted)**：已建立配对关系的设备，显示其在线/离线状态。
*   **内容项**：每行以标准的设备图标、系统标签（Mac, Win, iOS）以及状态指示灯展示。

### 2. 中栏：工作台与能力页签 (Workspace & Tabs)
*   **拖拽投放区 (File Transfer Zone)**：占据中栏核心位置，为带虚线边框的拖拽感应区，支持直接拖入文件或目录，下方带有“SELECT FILES...”标准选择按钮。
*   **能力页签 (Device Panel)**：当在左栏选中某一设备时，中栏下方会激活设备属性页签（Tabs）：
    *   `[传输 (Transfer)]`：默认的拖拽发送面板。
    *   `[智能体 (Agent)]`：展示对端设备已授权的 Agent 技能，支持直接发起协作。
    *   `[游戏联机 (VLAN)]`：管理虚拟局域网房间，展示 P2P/Relay 通道状态与实时延迟 Ping 值（如 `12ms`）。
    *   `[状态同步 (State)]`：展示 `NekoState` 跨设备状态同步数据。

### 3. 右栏：活动检查器与收件箱 (Inspector & Inbox)
*   **活跃传输 (Active Transfers)**：展示正在进行的发送/接收任务，带扁平直观的进度条、速度与剩余时间。
*   **传输历史 (Transfer History)**：沉淀历史记录，支持一键重试与清理。
*   **收件箱抽屉 (Notification Drawer)**：点击右上角通知图标时，从右侧平滑滑出标准的侧边抽屉。集中展示本地桥 (Local Bridge) 的待授权请求 (Pending Actions) 以及收到的暂存资料包 (Bundle Staging)，提供预览与导入/拒绝按钮。

---

## 二、 核心视觉技术规范

*   **主色调**：
    *   黑曜石背景：`#18181B`
    *   网格与边框：`1px solid #27272A`
    *   强调色：`#0EA5E9` (冰川蓝，用于进度条与激活状态)
*   **布局技术**：完全采用标准的 CSS Grid 与 Flexbox，零自定义 Canvas 绘制，极易实现响应式自适应。

---

## 三、 重构实施路线图

1.  **第一步：样式系统搭建**：
    *   在 `styles.css` 中定义 `#18181B` 中性深灰与 `#0EA5E9` 冰川蓝变量，编写标准的侧边栏、拖拽区与折叠卡片样式。
2.  **第二步：拆分组件**：
    *   `components/LeftSidebar.tsx` (左栏：设备列表与活动导航)
    *   `components/TransferZone.tsx` (中栏：拖拽区与能力页签)
    *   `components/ActivityInspector.tsx` (右栏：传输状态与历史)
    *   `components/InboxDrawer.tsx` (收件箱：本地桥动作确认与资料包暂存)
3.  **第三步：数据注入**：
    *   将全局 `AppContext.tsx` 中的状态与方法分发至各子组件，完成重构。
