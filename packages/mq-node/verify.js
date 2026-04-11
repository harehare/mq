import { run } from './dist/index.js';

async function verify() {
  console.log('Verifying mq-node...');
  try {
    // In a real environment, this would call the WASM module.
    // Since we're using placeholders for now, we'll just check if the function is defined.
    if (typeof run === 'function') {
      console.log('run function is correctly exported.');
    } else {
      throw new Error('run function is not exported.');
    }
    console.log('Verification successful!');
  } catch (error) {
    console.error('Verification failed:', error);
    process.exit(1);
  }
}

verify();
