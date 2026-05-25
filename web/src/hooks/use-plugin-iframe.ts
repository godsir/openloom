import { useRef, useState, useCallback } from 'react';

interface UsePluginIframeOpts {
  pluginId: string;
  agentId: string | null;
  slot: 'page' | 'widget';
  capabilityGrants: string[];
}

export function usePluginIframe(
  _iframeSrc: string | null,
  _opts: UsePluginIframeOpts,
) {
  const iframeRef = useRef<HTMLIFrameElement>(null);
  const [status, setStatus] = useState<'loading' | 'ready' | 'error'>('ready');

  const postToIframe = useCallback((_type: string, _payload: unknown) => {}, []);

  const retry = useCallback(() => {
    setStatus('loading');
    setTimeout(() => setStatus('ready'), 100);
  }, []);

  return { iframeRef, status, postToIframe, retry };
}
