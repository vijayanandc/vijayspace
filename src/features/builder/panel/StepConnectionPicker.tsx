// The connection select of a step or trigger: an OAuth sign-in started here saves the flow first and returns to it.

import { ConnectionPicker } from '../../connections';
import { useOptionalBuilder } from '../BuilderContext';

interface Props {
  connector: string;
  value: string | null | undefined;
  onChange: (id: string | null) => void;
}

export function StepConnectionPicker(props: Props) {
  const builder = useOptionalBuilder();
  return <ConnectionPicker {...props} returnTo={builder ? `/flows/${builder.flowId}` : undefined} beforeRedirect={builder?.flush} />;
}
