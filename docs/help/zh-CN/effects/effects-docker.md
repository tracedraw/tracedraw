# 效果面板

一个对象可以同时带有多个实时效果，依次应用。[[docker.effects]] 面板按应用顺序从上到下列出所选对象的效果。使用 [[menu.window > menu.window.dockers > menu.window.dockers_effects]] 打开它。

![效果面板](../../img/effects-effects-docker.png)

## 管理效果 {#stack}

列表的每一行都有：

- 一个打开或关闭效果的复选框。关闭的效果保留其位置和设置，但不会被绘制、导出或打印；
- 效果的名称，关闭时显示为暗淡；
- 上下箭头，将效果移到更早或更晚应用，这会改变结果（例如封套的轮廓图，或轮廓图的封套）；
- 一个删除效果的叉号。

每项更改为一个撤消步骤。每种效果仍使用其自身的工具或面板进行编辑。

## 圆角与切片 {#outline-effects}

面板顶部图层效果中的两项会按绘制时的样子重建轮廓，对象本身保持不变（文本仍可编辑）：

- **[[effect.round_corners]]** 用一个半径圆化曲线、群组中曲线或文本字母的所有角。**最大** 只圆化比某角度更尖的角（180° 为全部），**角** 按钮选择全部、仅外角或仅内角。
- **[[effect.slice]]** 把对象切成平行条带：条数、切线角度和间隙；**偏移** 让每条沿切线滑动，交替或随机；**旋转** 让每条随机转动一点；**种子** 选择另一种随机变化；**圆角** 圆化每条的角。

[[menu.object.convert_to_curves]]（或拆分）把结果变成普通曲线。

## 其他效果命令 {#commands}

- [[menu.effects > menu.effects.flatten]] 将所选对象的效果变为普通对象。
- [[menu.effects.picker]] 显示可应用到所选内容的效果列表。
- [[menu.object > menu.object.clear_effect]] 删除所选内容的效果。
- [[menu.object > menu.object.copy_effect]] 从另一个对象复制效果，[[menu.object.clone_effect]] 创建跟随它的克隆；请参阅 [复制和克隆效果](copy-and-clone.md)。
