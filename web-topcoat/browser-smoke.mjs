// Small isolated Chrome/CDP harness shared by focused browser regressions.
import {spawn} from 'node:child_process';
import {createServer} from 'node:net';
import {mkdtempSync,rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import assert from 'node:assert/strict';
const delay=ms=>new Promise(done=>setTimeout(done,ms));
export async function withBrowser(url,run,{ready='document.querySelector(".paper")?.dataset.ready==="true"'}={}) {
  const portServer=createServer();await new Promise(done=>portServer.listen(0,'127.0.0.1',done));
  const port=portServer.address().port;await new Promise(done=>portServer.close(done));
  const profile=mkdtempSync(join(tmpdir(),'vellio-note-ai-chrome-'));
  const chrome=process.env.CHROME||'/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
  const browser=spawn(chrome,['--headless=new','--no-first-run','--no-default-browser-check','--disable-extensions',
    `--user-data-dir=${profile}`,`--remote-debugging-port=${port}`,'--window-size=1400,900',url],{stdio:'ignore'});
  let socket;
  try {
    let page;
    for(let i=0;i<100;i++) {
      try{page=(await(await fetch(`http://127.0.0.1:${port}/json/list`)).json()).find(p=>p.type==='page');if(page)break;}catch{}
      await delay(100);
    }
    assert(page,'Chrome remote debugger unavailable');socket=new WebSocket(page.webSocketDebuggerUrl);
    await new Promise((done,fail)=>{socket.onopen=done;socket.onerror=fail;});
    let id=0;const pending=new Map(),errors=[],listeners=new Map();
    const on=(method,listener)=>{
      if(!listeners.has(method))listeners.set(method,new Set());
      listeners.get(method).add(listener);
      return ()=>listeners.get(method)?.delete(listener);
    };
    socket.onmessage=event=>{const message=JSON.parse(event.data);
      if(pending.has(message.id)){pending.get(message.id)(message);pending.delete(message.id);}
      if(message.method==='Runtime.exceptionThrown')errors.push(message.params.exceptionDetails.exception?.description||message.params.exceptionDetails.text);
      for(const listener of listeners.get(message.method)||[])listener(message.params);
    };
    const send=(method,params={})=>new Promise(done=>{const next=++id;pending.set(next,done);socket.send(JSON.stringify({id:next,method,params}));});
    const evaluate=async expression=>{
      const response=(await send('Runtime.evaluate',{expression,awaitPromise:true,returnByValue:true})).result;
      assert(!response.exceptionDetails,response.exceptionDetails?.exception?.description||response.exceptionDetails?.text);
      return response.result.value;
    };
    const wait=async expression=>{for(let i=0;i<150;i++){if(await evaluate(expression))return;await delay(80);}assert.fail(`browser timeout: ${expression}`);};
    await send('Runtime.enable');await wait(ready);
    await run({evaluate,send,wait,on});assert.deepEqual(errors,[]);
  } finally {
    socket?.close();browser.kill();await delay(250);rmSync(profile,{recursive:true,force:true,maxRetries:8,retryDelay:100});
  }
}
