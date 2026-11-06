#!/usr/bin/env python3
"""Local ChronoSync cluster partition matrix.

Runs several ChronoSync processes on distinct loopback IPs/ports. It exercises:
1. unicast-only discovery (broadcast effectively isolated by different ports)
2. split brain with two isolated auto servers
3. partition heal by restarting one node with a seed to the other
4. server loss and re-election

It never enables auto_apply or NTP 123, so it never writes a clock.
"""
import argparse, json, os, signal, subprocess, sys, tempfile, time, urllib.request

def http_json(port, path='/api/cluster/state'):
    with urllib.request.urlopen(f'http://127.0.0.1:{port}{path}', timeout=3) as r:
        return json.loads(r.read().decode())

def wait_state(port, predicate, timeout=12):
    deadline = time.time() + timeout
    last = None
    while time.time() < deadline:
        try:
            last = http_json(port)
            if predicate(last): return last
        except Exception: pass
        time.sleep(.3)
    return last

def start(binary, label, ip, web, discovery, time_port, mode, seed, root, secret):
    args=[binary,'--bind','127.0.0.1','--port',str(web),
          '--log-dir',os.path.join(root,label),
          '--cluster-mode',mode,'--cluster-bind',ip,'--cluster-id','matrix',
          '--node-id',label,'--cluster-secret-file',secret,
          '--discovery-port',str(discovery),'--time-port',str(time_port),
          '--server-eligible','true']
    if seed: args += ['--cluster-seed',seed]
    log=open(os.path.join(root,label+'.log'),'wb')
    p=subprocess.Popen(args,stdout=log,stderr=subprocess.STDOUT)
    return p,log

def stop(p):
    if p and p.poll() is None:
        p.terminate()
        try: p.wait(timeout=5)
        except subprocess.TimeoutExpired: p.kill(); p.wait()

def main():
    ap=argparse.ArgumentParser()
    ap.add_argument('--binary',default='target/release/chronosync')
    ap.add_argument('--out',default='')
    a=ap.parse_args()
    binary=os.path.abspath(a.binary)
    if not os.path.isfile(binary): sys.exit(f'binary not found: {binary}')
    root=a.out or tempfile.mkdtemp(prefix='chronosync-matrix-')
    os.makedirs(root,exist_ok=True)
    secret=os.path.join(root,'cluster.key')
    with open(secret,'w') as f: f.write('0123456789abcdef0123456789abcdef\n')
    os.chmod(secret,0o600)
    results={}
    ps=[]
    def record(name, ok, detail):
        results[name]={'ok':ok,'detail':detail}
        print(json.dumps({name:results[name]},ensure_ascii=False),flush=True)
    try:
        # 1. unicast/seed-only: A server, B client, discovery ports differ
        pa,la=start(binary,'seed-a01','127.0.0.10',19700,19600,19610,'server',None,root,secret)
        pb,lb=start(binary,'seed-b01','127.0.0.11',19701,19601,19611,'client','127.0.0.10:19600',root,secret)
        ps += [(pa,la),(pb,lb)]
        sa=wait_state(19700,lambda x:x.get('role')=='server')
        sb=wait_state(19701,lambda x:x.get('server_id')=='seed-a01' and (x.get('last_sync') or {}).get('offset_ms') is not None)
        record('broadcast_blocked_seed', bool(sa and sb and sb.get('server_id')=='seed-a01' and sb.get('last_error') is None),
               {'a_role':sa.get('role') if sa else None,'b_role':sb.get('role') if sb else None,'b_server':sb.get('server_id') if sb else None,'b_sync':sb.get('last_sync') if sb else None})
        stop(pa); stop(pb); ps=[]

        # 2. split brain: both auto, discovery ports isolated, no seeds
        pa,la=start(binary,'split-a1','127.0.0.10',19710,19620,19630,'auto',None,root,secret)
        pb,lb=start(binary,'split-b1','127.0.0.11',19711,19621,19631,'auto',None,root,secret)
        ps += [(pa,la),(pb,lb)]
        sa=wait_state(19710,lambda x:x.get('role')=='server',12)
        sb=wait_state(19711,lambda x:x.get('role')=='server',12)
        record('split_brain_two_servers', bool(sa and sb and sa.get('role')=='server' and sb.get('role')=='server'),
               {'a_role':sa.get('role') if sa else None,'b_role':sb.get('role') if sb else None,'a_term':sa.get('term') if sa else None,'b_term':sb.get('term') if sb else None})

        # 3. heal: restart B with A's discovery/time ports and seed A
        stop(pb); ps=[(pa,la)]
        pb,lb=start(binary,'split-b1','127.0.0.11',19711,19620,19630,'auto','127.0.0.10:19620',root,secret)
        ps.append((pb,lb))
        sa=wait_state(19710,lambda x:x.get('role')=='server')
        sb=wait_state(19711,lambda x:x.get('server_id')=='split-a1')
        record('partition_heal', bool(sa and sb and sb.get('server_id')=='split-a1' and sb.get('role')=='client'),
               {'a_role':sa.get('role') if sa else None,'b_role':sb.get('role') if sb else None,'b_server':sb.get('server_id') if sb else None})

        # 4. server loss: stop A, B should become server
        stop(pa); ps=[(pb,lb)]
        sb=wait_state(19711,lambda x:x.get('role')=='server',12)
        record('server_loss_reelection', bool(sb and sb.get('role')=='server'),
               {'b_role':sb.get('role') if sb else None,'b_term':sb.get('term') if sb else None})
    finally:
        for p,_ in ps: stop(p)
    return 0 if all(x['ok'] for x in results.values()) else 1

if __name__=='__main__':
    raise SystemExit(main())
