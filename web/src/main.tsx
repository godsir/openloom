// main.tsx — React 入口
// i18n 必须在 React render 之前初始化
import { initI18n } from './lib/i18n';
initI18n();

// 基础样式 + 动画
import './styles.css';
import './animations.css';

// 主题 CSS（[data-theme="xxx"] 选择器驱动，按 data-theme 属性激活）
import './themes/new-warm-paper-fonts.css';
import './themes/new-warm-paper.css';
import './themes/warm-paper.css';
import './themes/midnight.css';
import './themes/midnight-contrast.css';
import './themes/grass-aroma.css';
import './themes/deep-think.css';
import './themes/delve.css';
import './themes/absolutely.css';
import './themes/contemplation.css';
import './themes/high-contrast.css';

import { createRoot } from 'react-dom/client';
import App from './App';

const el = document.getElementById('react-root');
if (el) {
  createRoot(el).render(<App />);
}
