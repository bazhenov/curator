import React from 'react';
import ReactDOM from 'react-dom';
import './index.css';
import {App, Curator} from './App';
import * as serviceWorker from './serviceWorker';

let curator = new Curator();

ReactDOM.render(
  <React.StrictMode>
    <App curator={curator} />
  </React.StrictMode>,
  document.getElementById('root')
);

// If you want your app to work offline and load faster, you can change
// unregister() to register() below. Note this comes with some pitfalls.
// Learn more about service workers: https://bit.ly/CRA-PWA
serviceWorker.unregister();