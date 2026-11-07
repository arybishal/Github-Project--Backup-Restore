import { createContext, useContext } from 'react';

export type PageId =
  | 'dashboard'
  | 'repositories'
  | 'backup'
  | 'restore'
  | 'history'
  | 'local-projects'
  | 'publisher'
  | 'storage'
  | 'tasks'
  | 'logs'
  | 'settings'
  | 'about';

export interface NavValue {
  page: PageId;
  navigate: (page: PageId) => void;
}

export const NavContext = createContext<NavValue>({
  page: 'dashboard',
  navigate: () => {},
});

export function useNav(): NavValue {
  return useContext(NavContext);
}
