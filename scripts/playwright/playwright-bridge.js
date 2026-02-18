#!/usr/bin/env node
/**
 * Playwright Bridge - JSON-RPC bridge for browser automation
 * Communicates via stdin/stdout with newline-delimited JSON
 * 
 * Usage: node playwright-bridge.js [--browser chromium|firefox|webkit] [--headless]
 */

const readline = require('readline');
const { chromium, firefox, webkit } = require('playwright');

let browser = null;
let context = null;
let page = null;
let browserType = 'chromium';
let headless = true;

function logError(msg) {
    console.error(JSON.stringify({ error: msg }));
}

async function initBrowser() {
    if (browser) return true;
    
    const browserLauncher = browserType === 'firefox' ? firefox 
        : browserType === 'webkit' ? webkit 
        : chromium;
    
    try {
        browser = await browserLauncher.launch({ headless });
        context = await browser.newContext();
        page = await context.newPage();
        return true;
    } catch (e) {
        logError(`Failed to launch browser: ${e.message}`);
        return false;
    }
}

async function handleRequest(req) {
    const { id, action, args } = req;
    
    try {
        let result;
        
        switch (action) {
            case 'new_page':
                if (!await initBrowser()) {
                    return { id, success: false, error: 'Failed to initialize browser' };
                }
                return { id, success: true, data: { pageId: 1 } };
                
            case 'navigate':
                if (!page) return { id, success: false, error: 'No page available' };
                await page.goto(args.url, { waitUntil: 'domcontentloaded', timeout: 30000 });
                return { id, success: true, data: { url: page.url() } };
                
            case 'screenshot':
                if (!page) return { id, success: false, error: 'No page available' };
                const fullPage = args.fullPage !== false;
                const buffer = await page.screenshot({ 
                    fullPage,
                    type: args.format || 'png'
                });
                return { id, success: true, data: { 
                    image: buffer.toString('base64'),
                    format: args.format || 'png'
                }};
                
            case 'click':
                if (!page) return { id, success: false, error: 'No page available' };
                await page.click(args.selector, { timeout: 5000 });
                return { id, success: true };
                
            case 'fill':
                if (!page) return { id, success: false, error: 'No page available' };
                await page.fill(args.selector, args.value, { timeout: 5000 });
                return { id, success: true };
                
            case 'type':
                if (!page) return { id, success: false, error: 'No page available' };
                await page.type(args.selector, args.text, { delay: args.delay || 50 });
                return { id, success: true };
                
            case 'press':
                if (!page) return { id, success: false, error: 'No page available' };
                await page.press(args.selector, args.key);
                return { id, success: true };
                
            case 'hover':
                if (!page) return { id, success: false, error: 'No page available' };
                await page.hover(args.selector, { timeout: 5000 });
                return { id, success: true };
                
            case 'wait_for_selector':
                if (!page) return { id, success: false, error: 'No page available' };
                await page.waitForSelector(args.selector, { 
                    timeout: args.timeout || 30000,
                    state: args.state || 'visible'
                });
                return { id, success: true };
                
            case 'wait_for_text':
                if (!page) return { id, success: false, error: 'No page available' };
                await page.waitForFunction(
                    text => document.body.innerText.includes(text),
                    args.text,
                    { timeout: args.timeout || 30000 }
                );
                return { id, success: true };
                
            case 'evaluate':
                if (!page) return { id, success: false, error: 'No page available' };
                const evalResult = await page.evaluate(args.script);
                return { id, success: true, data: evalResult };
                
            case 'get_text':
                if (!page) return { id, success: false, error: 'No page available' };
                const text = await page.textContent(args.selector || 'body');
                return { id, success: true, data: { text } };
                
            case 'get_html':
                if (!page) return { id, success: false, error: 'No page available' };
                const html = await page.content();
                return { id, success: true, data: { html } };
                
            case 'get_url':
                if (!page) return { id, success: false, error: 'No page available' };
                return { id, success: true, data: { url: page.url() } };
                
            case 'get_title':
                if (!page) return { id, success: false, error: 'No page available' };
                const title = await page.title();
                return { id, success: true, data: { title } };
                
            case 'query_selector_all':
                if (!page) return { id, success: false, error: 'No page available' };
                const elements = await page.$$(args.selector);
                const count = elements.length;
                return { id, success: true, data: { count } };
                
            case 'set_viewport':
                if (!page) return { id, success: false, error: 'No page available' };
                await page.setViewportSize({ 
                    width: args.width || 1920, 
                    height: args.height || 1080 
                });
                return { id, success: true };
                
            case 'go_back':
                if (!page) return { id, success: false, error: 'No page available' };
                await page.goBack();
                return { id, success: true, data: { url: page.url() } };
                
            case 'go_forward':
                if (!page) return { id, success: false, error: 'No page available' };
                await page.goForward();
                return { id, success: true, data: { url: page.url() } };
                
            case 'reload':
                if (!page) return { id, success: false, error: 'No page available' };
                await page.reload();
                return { id, success: true, data: { url: page.url() } };
                
            case 'close':
                if (browser) {
                    await browser.close();
                    browser = null;
                    context = null;
                    page = null;
                }
                return { id, success: true };
                
            case 'ping':
                return { id, success: true, data: { pong: true } };
                
            default:
                return { id, success: false, error: `Unknown action: ${action}` };
        }
    } catch (e) {
        return { id, success: false, error: e.message };
    }
}

async function main() {
    const args = process.argv.slice(2);
    
    for (let i = 0; i < args.length; i++) {
        if (args[i] === '--browser' && args[i + 1]) {
            browserType = args[i + 1];
            i++;
        } else if (args[i] === '--headless') {
            headless = true;
        } else if (args[i] === '--no-headless' || args[i] === '--headed') {
            headless = false;
        }
    }
    
    const rl = readline.createInterface({
        input: process.stdin,
        output: process.stdout,
        terminal: false
    });
    
    rl.on('line', async (line) => {
        try {
            const req = JSON.parse(line);
            const response = await handleRequest(req);
            console.log(JSON.stringify(response));
        } catch (e) {
            console.log(JSON.stringify({ error: e.message, success: false }));
        }
    });
    
    process.on('SIGINT', async () => {
        if (browser) await browser.close();
        process.exit(0);
    });
    
    process.on('SIGTERM', async () => {
        if (browser) await browser.close();
        process.exit(0);
    });
}

main();
