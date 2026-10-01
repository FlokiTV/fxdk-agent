import { mount } from 'svelte';
import App from './App.svelte';

const target = document.getElementById('app');

if (!target) {
  throw new Error('FXDK Agent desktop root element was not found');
}

mount(App, { target });
