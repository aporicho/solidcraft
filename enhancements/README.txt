SolidCraft enhancements — aporicho/solidcraft

这是基于官方 v0.30.1 的独立 fork，不是官方可加载插件。
原版安装文件与增强版分别保留。用户模型格式没有改变。

分支：enhancements
上游：upstream -> tangency-labs/solidcraft（禁止推送）
自己的仓库：origin -> aporicho/solidcraft
基线：v0.30.1 / 8e2b0cd60d14b4152b12b8f1fdb2bcfdcd8c52f4

增强：固定毫米网格、捕捉范围、圆心拖动网格、在线与网格同时吸附。
模型页新增“选择增强”：环选、外边缘。
外边缘：先选平面，再点击图标，只选最外圈，不包含内部孔、槽的内圈。
环选：先选边，再点击图标，补齐该边所在的一圈；可用于孔口。
一条边有两个相邻面时优先使用面积较大的平面，也可同时选面明确指定。
当前只支持平面上的闭合轮廓，不把三角网格线作为可倒角的真实边。
选择完成后可直接打开倒圆角或倒角，预选边会被带入。

构建：sh enhancements/build.sh
完整检查：sh enhancements/check.sh（限制并发，避免占满桌面电脑内存）
源代码增量包：先 git add 新增文件，再 sh enhancements/package.sh。
补丁包基于精确的 v0.30.1；在新的官方源码副本中校验 SHA256SUMS，
然后 git apply --check changes.patch，确认后 git apply changes.patch，再构建。
不得向正在使用的原版安装文件直接应用源码补丁。

更新上游时先检查差异，在独立分支合并并验证，再更新 manifest 中的基线。
不要自动跨版本应用旧补丁，不向 upstream 推送，不改写自己的已发布历史。
新增图标保留 Open 状态供用户审阅；未修改官方已接受的图标。
