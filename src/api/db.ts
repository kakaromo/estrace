import Database from '@tauri-apps/plugin-sql';
import { setting } from "$stores/setting";
import type { TestInfo } from "$stores/trace";
import { platform } from '@tauri-apps/plugin-os';
import { join, homeDir } from '@tauri-apps/api/path';

let db: Database = null;

async function getDbPath() {
    const currentPlatform = platform();
    if (currentPlatform === 'windows') {
        return 'sqlite:test.db';
    } else {
        // Linux 또는 macOS
        const home = await homeDir();
        return `sqlite://${await join(home, 'test.db')}`;
    }
}

async function open() {
    if(!db) {
        const dbPath = await getDbPath();
        db = await Database.load(dbPath);    
    }
    return db;
}

export async function initial() {    
    await open();
    await db.execute('CREATE TABLE IF NOT EXISTS setting (id INTEGER PRIMARY KEY, value TEXT);').catch((e) => { console.log('error', e); });
    await db.execute('CREATE TABLE IF NOT EXISTS app (id INTEGER PRIMARY KEY, filename TEXT);').catch((e) => { console.log('error', e); });
    await db.execute('CREATE TABLE IF NOT EXISTS folder (id INTEGER PRIMARY KEY, path TEXT);').catch((e) => { console.log('error', e); });
    await db.execute('CREATE TABLE IF NOT EXISTS file (id INTEGER PRIMARY KEY, path TEXT);').catch((e) => { console.log('error', e); });
    await db.execute('CREATE TABLE IF NOT EXISTS log (id INTEGER PRIMARY KEY, path TEXT);').catch((e) => { console.log('error', e); });
    await db.execute('CREATE TABLE IF NOT EXISTS testinfo (id INTEGER PRIMARY KEY, logtype TEXT, title TEXT, content TEXT, logfolder TEXT, logname TEXT);').catch((e) => { console.log('error', e); });
    await db.execute('CREATE TABLE IF NOT EXISTS testtype (id INTEGER PRIMARY KEY, path TEXT);').catch((e) => { console.log('error', e); });    

}

export async function getFolder() {
    await open();
    const result:TestInfo[] = await db.select('SELECT path FROM folder WHERE id = 1');
    
    setting.update((s) => {
        if(result.length > 0) {
            s.logfolder = result[0].path;
        }
        return s;
    });
    
    return result;
}

export async function setFolder(key: string, value: string) {
    await open();
    await db.execute('INSERT OR REPLACE INTO folder (id, path) VALUES (1, ?)', [value]);
    setting.update((s) => {
        s[key] = value;
        return s;
    });
}

export async function getAllTestInfo() {
    await open();
    const result:TestInfo[] = await db.select('SELECT * FROM testinfo');
    return result;
}

export async function getTestInfo(id: number) {
    await open();
    const result:TestInfo[] = await db.select('SELECT * FROM testinfo WHERE id = ?', [id]);
    return result.length === 0? null : result[0];
}

export async function setTestInfo(logtype: string, title: string, content: string, logfolder: string, logname: string) {
    await open();
    await db.execute('INSERT INTO testinfo (logtype, title, content, logfolder, logname) VALUES (?, ?, ?, ?, ?)', [ logtype, title, content, logfolder, logname ]);
}
