/**
 * Sample branching graph for the WeChat minigame template.
 * Replace this object when exporting a generated game; keep node.video as a
 * packaged relative path or an https URL.
 *
 * videoBaseUrl: empty plays files copied from media/ into USER_DATA_PATH.
 * Set it to a legally-configured CDN origin to stream node videos instead.
 */
module.exports = {
  name: '雾城抉择',
  description: '在雾城醒来的林岩，必须判断谁值得相信。每一次选择都会留下路径。',
  videoBaseUrl: '',
  nodes: [
    { id: 'start', type: 'start', title: '雾中醒来', text: '林岩在废弃车站醒来，远处传来一阵短促的铃声。', video: 'media/01-start.mp4' },
    { id: 'bridge', type: 'normal', title: '桥下的灯', text: '雾气在桥洞下聚拢，一盏旧灯忽明忽暗。', video: 'media/02-bridge.mp4' },
    { id: 'archive', type: 'normal', title: '墙上的纸条', text: '砖墙上贴着一张湿透的纸条，墨迹指向旧档案馆。', video: 'media/03-archive.mp4' },
    { id: 'station', type: 'normal', title: '无声的站台', text: '两条线索最终都把林岩带回了没有列车的站台。', video: 'media/04-station.mp4' },
    { id: 'truth', type: 'success', title: '看见天光', text: '林岩打开档案盒，找到了同伴留下的完整证据，雾城终于迎来天光。', video: 'media/05-truth.mp4' },
    { id: 'dawn', type: 'success', title: '带她离开', text: '林岩选择相信铃声背后的人，和同伴在第一班晨车到来前离开了雾城。', video: 'media/06-dawn.mp4' },
    { id: 'echo', type: 'failure', title: '回声尽头', text: '错误的脚步声在隧道里反复回荡，林岩错过了唯一的出口。', video: 'media/07-echo.mp4' },
    { id: 'silence', type: 'failure', title: '沉入雾中', text: '纸条被撕碎，线索消失在雨水里，雾城再次恢复了沉默。', video: 'media/08-silence.mp4' },
  ],
  edges: [
    { from: 'start', to: 'bridge', text: '跟随桥下的灯' },
    { from: 'start', to: 'archive', text: '检查墙上的纸条' },
    { from: 'bridge', to: 'station', text: '记住铃声的节奏' },
    { from: 'bridge', to: 'echo', text: '直接走进隧道' },
    { from: 'archive', to: 'station', text: '把纸条带回站台' },
    { from: 'archive', to: 'silence', text: '撕掉纸条离开' },
    { from: 'station', to: 'truth', text: '打开旧档案盒' },
    { from: 'station', to: 'dawn', text: '相信远处的晨铃' },
  ],
};
