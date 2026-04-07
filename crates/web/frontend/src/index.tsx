import { render } from 'solid-js/web';
import App from './App';
import './styles.css';
import { initKeyboard } from './utils/keyboard';

initKeyboard();
render(() => <App />, document.getElementById('root')!);
